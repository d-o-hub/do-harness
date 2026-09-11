//! Filesystem permission helpers shared by `init` and hook installation.

use std::path::Path;

use anyhow::Result;

/// Owner execute bit applied to installed scripts and hooks (unix only).
#[cfg(unix)]
pub(crate) const OWNER_EXEC_MASK: u32 = 0o111;

/// Adds owner execute permission to `path` (unix only; no-op elsewhere).
///
/// # Errors
///
/// Returns an error when the path cannot be stat'ed or chmod'ed.
#[cfg(unix)]
pub(crate) fn set_owner_exec(path: &Path) -> Result<()> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use anyhow::Context;

    let permissions = fs::metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .permissions();
    let mode = permissions.mode() | OWNER_EXEC_MASK;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("failed to chmod {}", path.display()))
}

/// No-op on non-unix platforms.
///
/// # Errors
///
/// Never returns an error.
#[cfg(not(unix))]
pub(crate) fn set_owner_exec(_path: &Path) -> Result<()> {
    Ok(())
}
