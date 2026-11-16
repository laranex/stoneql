//! Authorized-file policy shared by SSH key files and (when built) SQLite files. No driver code.
use super::LifecycleError;
use std::path::{Path, PathBuf};

/// Accepts only an absolute, already-canonical path to an existing regular file. Relative
/// paths, `..` components, symlinks (final or intermediate) and special files are refused.
pub(crate) fn authorize(path: &Path) -> Result<PathBuf, LifecycleError> {
    let text = path.to_str().ok_or(LifecycleError::InvalidConfiguration)?;
    if !path.is_absolute() || text.contains('\0') || text.len() > 4096 {
        return Err(LifecycleError::InvalidConfiguration);
    }
    verify(path)?;
    Ok(path.to_path_buf())
}
/// Re-run at every open: a granted path later replaced by a symlink or directory is refused.
/// SQLite still opens by path, so a same-user race between this check and open remains.
pub(crate) fn verify(path: &Path) -> Result<(), LifecycleError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| LifecycleError::Forbidden)?;
    let canonical = std::fs::canonicalize(path).map_err(|_| LifecycleError::Forbidden)?;
    if !metadata.file_type().is_file() || canonical != path {
        return Err(LifecycleError::Forbidden);
    }
    Ok(())
}
