//! Hermetic sandbox construction for `do-harness eval`.
//!
//! # Sandbox boundary (be explicit)
//!
//! "Hermetic" here means filesystem isolation only: the walkthrough and every
//! graded assertion run against a `tempfile` directory, so residue cannot
//! touch the caller's repository. The child process still runs with the
//! caller's privileges and full syscall access — there is **no** seccomp
//! filter, network namespace, cgroup, or gVisor-style sandbox. Do not run
//! walkthroughs from untrusted skills without an outer sandbox. The stderr
//! tail captured on failure is an observability bound, not containment.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// A hermetic sandbox that mirrors a skill plus skill-creator into a temp dir.
///
/// The temp root becomes the workspace root for the walkthrough and every
/// graded assertion, so residue lands under the temp dir and the caller's
/// repository stays untouched.
pub(super) struct Sandbox {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Sandbox {
    /// Copies `skill_dir` (SKILL.md + evals) and skill-creator into a fresh
    /// temp root shaped like a harness workspace.
    ///
    /// Only the repository paths a skill's own files actually name are
    /// mirrored (`scripts/publish-npm.sh`, `docs/releasing.md`, ...), never
    /// whole trees: agent mode builds one sandbox per case, so copying
    /// `integrations/` wholesale would move hundreds of kilobytes per case for
    /// files no assertion reads.
    ///
    /// Coupling to be aware of: mirrored paths are non-hidden entries, and
    /// `init::detect` falls back to the generic pack once the root has any. A
    /// skill that both names repo paths and runs `do-harness init` in its
    /// walkthrough would therefore measure generic-pack behavior; keep those
    /// two activities in separate skills.
    pub(super) fn for_skill(real_root: &Path, skill_dir: &Path, name: &str) -> Result<Sandbox> {
        let dir = tempfile::tempdir().context("failed to create eval sandbox")?;
        let root = dir.path().to_path_buf();
        let skills_root = root.join(".agents").join("skills");
        let dest_skill = skills_root.join(name);
        copy_gate_scripts(real_root, &skills_root)?;
        for rel in referenced_paths(skill_dir) {
            let src = real_root.join(&rel);
            let dest = root.join(&rel);
            if src.is_dir() {
                copy_if_dir(&src, &dest)?;
            } else if src.is_file() {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("failed to create {}", parent.display()))?;
                }
                fs::copy(&src, &dest).with_context(|| {
                    format!("failed to copy {} -> {}", src.display(), dest.display())
                })?;
            }
        }
        copy_dir(skill_dir, &dest_skill)?;
        Ok(Sandbox { _dir: dir, root })
    }

    /// Path of the hermetic workspace root.
    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    /// Strips the guidance payload (`SKILL.md` + `references/`) from the
    /// mirrored skill, leaving `evals/` (the task) and `scripts/` (the
    /// tooling) so the identical walkthrough and assertions can run as the
    /// without-skill baseline. Skill Lift is the with-skill score minus the
    /// score measured here.
    pub(super) fn strip_guidance(&self, name: &str) -> Result<()> {
        let skill = self.root.join(".agents").join("skills").join(name);
        for payload in [skill.join("SKILL.md"), skill.join("references")] {
            if payload.is_file() || payload.is_symlink() {
                fs::remove_file(&payload)
                    .with_context(|| format!("failed to strip {}", payload.display()))?;
            } else if payload.is_dir() {
                fs::remove_dir_all(&payload)
                    .with_context(|| format!("failed to strip {}", payload.display()))?;
            }
        }
        Ok(())
    }

    /// Whether the skill's guidance payload is present again.
    ///
    /// The without-skill baseline strips guidance before running, but a
    /// walkthrough that bootstraps a workspace (for example `do-harness init`)
    /// can regenerate `SKILL.md` from the scaffolded template. The baseline
    /// then grades a copy of the very guidance it was supposed to measure
    /// without, so lift collapses toward zero and understates the skill. The
    /// caller checks this after the baseline run and reports lift as
    /// unmeasurable rather than publishing a contaminated number.
    pub(super) fn guidance_present(&self, name: &str) -> bool {
        let skill = self.root.join(".agents").join("skills").join(name);
        let skill_md = skill.join("SKILL.md");
        skill_md.is_file() || skill_md.is_symlink() || skill.join("references").is_dir()
    }

    /// Path of the copied skill-creator gate script within the sandbox.
    pub(super) fn gate_script(&self) -> PathBuf {
        self.root
            .join(".agents")
            .join("skills")
            .join("skill-creator")
            .join("scripts")
            .join("quick_validate.py")
    }
}

/// Mirrors skill-creator's `scripts/` directory into the sandbox recursively,
/// so nested helper directories do not abort the eval run.
fn copy_gate_scripts(real_root: &Path, skills_root: &Path) -> Result<()> {
    let gate_src = real_root
        .join(".agents")
        .join("skills")
        .join("skill-creator")
        .join("scripts");
    if !gate_src.is_dir() {
        return Ok(());
    }
    let dest_scripts = skills_root.join("skill-creator").join("scripts");
    fs::create_dir_all(&dest_scripts)
        .with_context(|| format!("failed to create {}", dest_scripts.display()))?;
    copy_dir(&gate_src, &dest_scripts)
}

/// Repository-relative paths a skill's own files name, so only those are
/// mirrored into its sandbox.
///
/// Scans the skill's text for `scripts/...`, `docs/...`, and
/// `integrations/...` tokens. Whole-tree copies are deliberately avoided (agent
/// mode builds one sandbox per case), but the granularity differs by tree:
///
/// * `scripts/...` and `docs/...` tokens mirror the exact named file.
/// * `integrations/<pkg>/...` mirrors `integrations/<pkg>` wholesale, because an
///   `integrations/` entry is a self-contained package whose manifest defines
///   its own file closure. A repo script the skill invokes (e.g.
///   `scripts/publish-npm.sh`) reads that package's `platforms/`, `bin/`, and
///   `package.json`, none of which the skill text enumerates — copying only the
///   named leaf would break the real tool the skill is exercising.
///
/// Bare tree prefixes (`scripts/`) are dropped: a whole-tree copy would pull in
/// unrelated tools and, being non-hidden, could change what `init::detect` sees.
pub(super) fn referenced_paths(skill_dir: &Path) -> Vec<String> {
    const PREFIXES: [&str; 3] = ["scripts/", "docs/", "integrations/"];
    let mut found: Vec<String> = Vec::new();
    let mut stack = vec![skill_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            for raw in text.split(|c: char| c.is_whitespace() || c == '`' || c == '"' || c == '\'')
            {
                let token = raw.trim_matches(|c: char| {
                    c == '(' || c == ')' || c == ',' || c == '.' || c == ':' || c == '|'
                });
                let Some(start) = PREFIXES.iter().find_map(|prefix| token.find(prefix)) else {
                    continue;
                };
                let cleaned: String = token[start..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'))
                    .collect();
                let cleaned = cleaned.trim_end_matches(['/', '.', '-']).to_owned();
                // Require something below a known tree: a bare prefix names no
                // concrete artifact.
                let Some((head, rest)) = cleaned.split_once('/') else {
                    continue;
                };
                if rest.is_empty() {
                    continue;
                }
                let mirror = if head == "integrations" {
                    match rest.split_once('/') {
                        Some((pkg, _)) if !pkg.is_empty() => format!("{head}/{pkg}"),
                        _ => cleaned.clone(),
                    }
                } else {
                    cleaned.clone()
                };
                if !found.contains(&mirror) {
                    found.push(mirror);
                }
            }
        }
    }
    found.sort();
    found
}

/// Copies `src` into `dest` when `src` is a directory; a missing source is not
/// an error so a skill can be evaluated in a workspace without that tree.
fn copy_if_dir(src: &Path, dest: &Path) -> Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    copy_dir(src, dest)
}

/// Recursively copies `src` into `dest`.
fn copy_dir(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest).with_context(|| format!("failed to create {}", dest.display()))?;
    for entry in fs::read_dir(src).with_context(|| format!("failed to read {}", src.display()))? {
        let entry = entry?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            fs::copy(&from, &to).with_context(|| {
                format!("failed to copy {} -> {}", from.display(), to.display())
            })?;
        }
    }
    Ok(())
}
