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
///
/// The `Result` is deliberate: it keeps this signature interchangeable with the
/// unix implementation, so callers write one `?` on every platform rather than
/// a `#[cfg]` at each call site. `clippy::unnecessary_wraps` cannot see the other
/// arm, so it is allowed here.
#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn set_owner_exec(_path: &Path) -> Result<()> {
    Ok(())
}

/// Whether `path` carries an execute bit, so git will actually run it.
///
/// Always true off unix, where git does not consult the file mode.
#[cfg(unix)]
#[must_use]
pub(crate) fn is_owner_exec(path: &Path) -> bool {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path).is_ok_and(|meta| meta.permissions().mode() & OWNER_EXEC_MASK != 0)
}

/// No-op success on non-unix platforms.
#[cfg(not(unix))]
#[must_use]
pub(crate) fn is_owner_exec(_path: &Path) -> bool {
    true
}
