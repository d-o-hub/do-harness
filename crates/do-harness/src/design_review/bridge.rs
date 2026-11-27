//! Host file-handoff bridge between the review adapter and a host-side
//! reviewer.
//!
//! The reviewer process contract (see [`super::reviewer`]) requires a
//! separate process that receives the packet on stdin and prints a findings
//! JSON object on stdout. This bridge satisfies that contract without
//! launching any model itself: it writes the packet verbatim to
//! `<handoff-dir>/packet.json` and waits for the operator's host to place
//! `<handoff-dir>/findings.json`, relaying that file verbatim on stdout. It
//! waits out partial writes (the file is accepted once it parses as JSON; a
//! response that never parses is relayed verbatim at the deadline so the
//! adapter classifies the invalid reviewer output as a failure). The bridge
//! reads nothing but the packet on stdin and the findings file, and echoes
//! neither the packet nor any host context.
//!
//! No findings file within `--max-wait-s` is an unavailable capability: the
//! bridge exits with [`super::reviewer::UNAVAILABLE_EXIT_CODE`] and
//! `capability unavailable` on stderr, the adapter's `not_run` contract,
//! which is never a successful review. An invalid packet on stdin is a usage
//! error (exit 2).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::reviewer::UNAVAILABLE_EXIT_CODE;

/// Poll cadence while waiting for the findings file.
const POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Default seconds to wait for a findings file.
pub const DEFAULT_MAX_WAIT_S: f64 = 570.0;

/// Runs the bridge: relay the stdin packet into the handoff directory and
/// wait for the findings file.
///
/// # Errors
///
/// A usage error (exit 2 under the dispatch mapping) when stdin is not a
/// JSON object or the handoff directory cannot be created or written. The
/// unavailable-capability path exits the process directly with the
/// reviewer-contract exit code 9, which no [`crate::CliError`] class
/// represents.
pub fn run(handoff_dir: &Path, max_wait_s: f64) -> Result<()> {
    let mut packet_raw = String::new();
    std::io::stdin()
        .read_to_string(&mut packet_raw)
        .context("failed to read the review packet from stdin")?;
    let packet: Value = serde_json::from_str(&packet_raw)
        .map_err(|_| anyhow::anyhow!("packet must be a JSON object"))?;
    if !packet.is_object() {
        bail!("packet must be a JSON object");
    }

    std::fs::create_dir_all(handoff_dir).with_context(|| {
        format!(
            "failed to create handoff directory {}",
            handoff_dir.display()
        )
    })?;
    let packet_path = handoff_dir.join("packet.json");
    std::fs::write(&packet_path, packet_raw.as_bytes())
        .with_context(|| format!("failed to write {}", packet_path.display()))?;

    let findings_path: PathBuf = handoff_dir.join("findings.json");
    let wait = max_wait_s.max(POLL_INTERVAL.as_secs_f64());
    let deadline = Instant::now() + Duration::from_secs_f64(wait);
    let mut relayed: Option<String> = None;
    while Instant::now() < deadline {
        if let Ok(text) = std::fs::read_to_string(&findings_path) {
            relayed = Some(text.clone());
            if serde_json::from_str::<Value>(&text).is_ok() {
                print!("{text}");
                return Ok(());
            }
        }
        std::thread::sleep(POLL_INTERVAL);
    }

    if let Some(text) = relayed {
        // The response never parsed: relay it verbatim so the adapter
        // classifies the invalid reviewer output as a failure.
        print!("{text}");
        return Ok(());
    }

    let mut stderr = std::io::stderr();
    let _ = writeln!(
        stderr,
        "capability unavailable: no findings file within {wait:.1}s"
    );
    let _ = std::io::stdout().flush();
    std::process::exit(UNAVAILABLE_EXIT_CODE);
}
