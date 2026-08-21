//! Hermetic sandbox construction for `do-harness eval`.

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
    /// Copies `skill_dir` (SKILL.md + evals) and skill-creator scripts into a
    /// fresh temp root shaped like a harness workspace.
    pub(super) fn for_skill(real_root: &Path, skill_dir: &Path, name: &str) -> Result<Sandbox> {
        let dir = tempfile::tempdir().context("failed to create eval sandbox")?;
        let root = dir.path().to_path_buf();
        let skills_root = root.join(".agents").join("skills");
        let dest_skill = skills_root.join(name);
        copy_gate_scripts(real_root, &skills_root)?;
        copy_dir(skill_dir, &dest_skill)?;
        Ok(Sandbox { _dir: dir, root })
    }

    /// Path of the hermetic workspace root.
    pub(super) fn root(&self) -> &Path {
        &self.root
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
