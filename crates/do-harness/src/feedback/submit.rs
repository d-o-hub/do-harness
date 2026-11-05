//! Issue submission, deduplication search, and receipt recording (WF-09).

use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use super::draft::validate_repo;
use super::types::{FeedbackDraft, SubmissionReceipt, SubmissionResult};
use crate::shell;

/// Wall-clock budget for one `gh` invocation.
const GH_TIMEOUT: Duration = Duration::from_secs(10);
/// Poll interval while waiting for `gh`.
const GH_POLL: Duration = Duration::from_millis(20);

/// Reads a child pipe to EOF on its own thread.
///
/// The reader must not block the wait loop: a child that fills a pipe would
/// otherwise never exit, and the timeout would fire on a healthy process.
fn read_pipe(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut buffer);
        }
        buffer
    })
}

/// Waits for `child`, killing its process tree once [`GH_TIMEOUT`] elapses.
fn wait_bounded(child: &mut Child) -> Result<ExitStatus> {
    let deadline = Instant::now() + GH_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() >= deadline => {
                shell::kill_tree(child);
                bail!("gh exceeded {GH_TIMEOUT:?}");
            }
            Ok(None) => std::thread::sleep(GH_POLL),
            Err(error) => bail!("wait for gh failed: {error}"),
        }
    }
}

/// Runs `gh` in a private process group with a bounded wall clock.
///
/// A hung `gh` must not hang a CLI that has already failed, so the child and
/// its descendants are killed when the budget expires.
fn run_gh(mut command: Command, body: Option<&str>) -> Result<Output> {
    shell::configure_process_group(&mut command);
    command
        .stdin(if body.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().context("spawn gh")?;
    // Drain the output pipes before writing the body: a child that fills a pipe
    // while we are still writing stdin would deadlock both sides.
    let stdout = read_pipe(child.stdout.take());
    let stderr = read_pipe(child.stderr.take());
    if let Some(body) = body {
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(error) = stdin.write_all(body.as_bytes()) {
                shell::kill_tree(&mut child);
                return Err(error).context("write gh stdin");
            }
        }
    }
    let status = wait_bounded(&mut child)?;
    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

/// Searches for an existing issue with the exact fingerprint marker.
fn find_existing_issue(repo: &str, fingerprint: &str) -> Result<Option<(u64, String)>> {
    let mut command = Command::new("gh");
    command.args([
        "issue",
        "list",
        "--repo",
        repo,
        "--state",
        "all",
        "--search",
        fingerprint,
        "--json",
        "number,url,state,title,body",
    ]);
    let output =
        run_gh(command, None).context("failed to execute gh issue list for deduplication")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh issue list failed: {stderr}");
    }

    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parse gh issue list json output")?;

    let Some(issues) = parsed.as_array() else {
        return Ok(None);
    };

    let marker = format!("<!-- do-harness-feedback-fingerprint: {fingerprint} -->");
    for issue in issues {
        let body = issue
            .get("body")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if body.contains(&marker) {
            let number = issue
                .get("number")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let url = issue
                .get("url")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned();
            if number > 0 && !url.is_empty() {
                return Ok(Some((number, url)));
            }
        }
    }

    Ok(None)
}

fn load_receipts(root: &Path) -> Vec<SubmissionReceipt> {
    let path = root.join(".do-harness/feedback/receipts.json");
    if let Ok(data) = fs::read(path) {
        serde_json::from_slice(&data).unwrap_or_default()
    } else {
        Vec::new()
    }
}

fn store_receipt(root: &Path, receipt: &SubmissionReceipt) -> Result<()> {
    let dir = root.join(".do-harness/feedback");
    fs::create_dir_all(&dir).context("create feedback state directory")?;
    let path = dir.join("receipts.json");
    let mut receipts = load_receipts(root);
    receipts.retain(|r| r.fingerprint != receipt.fingerprint);
    receipts.push(receipt.clone());
    let data = serde_json::to_vec_pretty(&receipts).context("serialize receipts")?;
    fs::write(path, data).context("write feedback receipts")?;
    Ok(())
}

/// Submits or links a feedback draft.
pub fn submit_draft(
    root: &Path,
    draft: &FeedbackDraft,
    explicit_repo: Option<&str>,
    skip_confirmation: bool,
    dry_run: bool,
) -> Result<SubmissionResult> {
    let repo = explicit_repo.unwrap_or(&draft.target_repo);
    validate_repo(repo)?;

    let local_receipts = load_receipts(root);
    if let Some(existing) = local_receipts
        .iter()
        .find(|r| r.fingerprint == draft.fingerprint)
    {
        return Ok(SubmissionResult::LinkedExisting {
            issue_number: existing.issue_number,
            issue_url: existing.issue_url.clone(),
        });
    }

    // Remote deduplication search across open and closed issues
    if let Some((number, url)) = find_existing_issue(repo, &draft.fingerprint)? {
        let receipt = SubmissionReceipt {
            fingerprint: draft.fingerprint.clone(),
            target_repo: repo.to_owned(),
            issue_number: number,
            issue_url: url.clone(),
            timestamp: do_harness_db::unix_now(),
            status: "linked_existing".to_owned(),
        };
        let _ = store_receipt(root, &receipt);
        return Ok(SubmissionResult::LinkedExisting {
            issue_number: number,
            issue_url: url,
        });
    }

    if dry_run {
        return Ok(SubmissionResult::DryRun {
            title: draft.title.clone(),
            target_repo: repo.to_owned(),
            would_create: true,
        });
    }

    if !skip_confirmation {
        bail!("submission requires explicit confirmation; re-run with --yes to publish to {repo}");
    }

    // Issue creation with fixed argv and stdin body
    let mut command = Command::new("gh");
    command.args([
        "issue",
        "create",
        "--repo",
        repo,
        "--title",
        &draft.title,
        "--body-file",
        "-",
    ]);
    let output = run_gh(command, Some(&draft.body)).context("spawn gh issue create")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh issue create failed: {stderr}");
    }

    let issue_url = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if issue_url.is_empty() {
        bail!("gh issue create succeeded but returned no issue URL");
    }

    let issue_number = issue_url
        .rsplit('/')
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let receipt = SubmissionReceipt {
        fingerprint: draft.fingerprint.clone(),
        target_repo: repo.to_owned(),
        issue_number,
        issue_url: issue_url.clone(),
        timestamp: do_harness_db::unix_now(),
        status: "created".to_owned(),
    };
    let _ = store_receipt(root, &receipt);

    Ok(SubmissionResult::Created {
        issue_number,
        issue_url,
    })
}
