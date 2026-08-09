//! Atomic file writer with fsync and rename guarantees.
//!
//! Implements the tmp + fsync + rename pattern specified in DD-003
//! and SECURITY.md. Ensures that output files are either fully
//! written or not visible at all, even after power loss or crash.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::error::{CueBladeError, Result};

/// Atomic file writer using tmp + fsync + rename.
///
/// Creates a temporary file in the same directory as the target,
/// writes data through a closure, calls `sync_all()`, then atomically
/// renames to the final path. On any error, the temp file is cleaned up.
///
/// # Security
///
/// - Output path is validated against base directory (no traversal).
/// - Symlinks in output path are rejected.
/// - Temp files use UUID v4 names to prevent collisions.
/// - Temp files inherit process umask (no world-writable outputs).
///
/// # Examples
///
/// ```no_run
/// use std::io::Write;
/// use cueblade::safety::AtomicWriter;
/// use std::path::Path;
///
/// let writer = AtomicWriter::new(
///     Path::new("/output"),
///     Path::new("album/01 - Track.flac"),
/// ).unwrap();
///
/// writer.write_with(|file| {
///     file.write_all(b"audio data")?;
///     Ok(())
/// }).unwrap();
/// // File is now atomically at /output/album/01 - Track.flac
/// ```
pub struct AtomicWriter {
    /// Base directory for output (all paths validated against this).
    base_dir: PathBuf,
    /// Relative path within base_dir for the final file.
    relative_path: PathBuf,
    /// Resolved absolute target path.
    target_path: PathBuf,
}

impl AtomicWriter {
    /// Create a new atomic writer for the given output path.
    ///
    /// `relative_path` is resolved against `base_dir`. The resulting
    /// absolute path must remain within `base_dir` (no `..` traversal).
    /// Symlinks in the output path are rejected.
    ///
    /// Parent directories are created automatically.
    ///
    /// # Errors
    ///
    /// - [`CueBladeError::InputValidation`] if path escapes base_dir.
    /// - [`CueBladeError::Io`] if parent directory creation fails.
    pub fn new(base_dir: &Path, relative_path: &Path) -> Result<Self> {
        // Canonicalize base_dir (must exist)
        let base_dir = fs::canonicalize(base_dir).map_err(|e| CueBladeError::Io {
            path: base_dir.to_path_buf(),
            source: e,
        })?;

        // Resolve target path without requiring it to exist yet
        let target_path = normalize_path(&base_dir.join(relative_path));

        // Validate: target must be within base_dir
        if !target_path.starts_with(&base_dir) {
            return Err(CueBladeError::InputValidation {
                reason: format!(
                    "Output path escapes base directory: {} is not within {}",
                    target_path.display(),
                    base_dir.display()
                ),
            });
        }

        // Reject symlinks in any component of the output path
        check_no_symlinks(&target_path, &base_dir)?;

        // Ensure parent directory exists
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|e| CueBladeError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }

        Ok(Self {
            base_dir,
            relative_path: relative_path.to_path_buf(),
            target_path,
        })
    }

    /// Write data atomically through a closure.
    ///
    /// The closure receives a `&mut File` pointing to a temporary file.
    /// After the closure returns `Ok(())`, `sync_all()` is called and
    /// the temp file is renamed to the target path.
    ///
    /// If the closure returns an error or `sync_all()`/`rename()` fails,
    /// the temporary file is removed.
    ///
    /// # Errors
    ///
    /// - [`CueBladeError::Io`] for any I/O failure.
    /// - Any error propagated from the closure via `?`.
    pub fn write_with<F>(self, f: F) -> Result<()>
    where
        F: FnOnce(&mut File) -> Result<()>,
    {
        let tmp_path = self.tmp_path();

        // Create temp file with restrictive permissions (inherits umask)
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true) // fail if exists (UUID collision guard)
            .open(&tmp_path)
            .map_err(|e| CueBladeError::Io {
                path: tmp_path.clone(),
                source: e,
            })?;

        // Execute user closure; clean up on error
        if let Err(e) = f(&mut file) {
            let _ = fs::remove_file(&tmp_path);
            return Err(e);
        }

        // Flush buffered writes before fsync
        if let Err(e) = file.flush() {
            let _ = fs::remove_file(&tmp_path);
            return Err(CueBladeError::Io {
                path: tmp_path.clone(),
                source: e,
            });
        }

        // fsync for durability (DD-003, SECURITY.md)
        if let Err(e) = file.sync_all() {
            let _ = fs::remove_file(&tmp_path);
            return Err(CueBladeError::Io {
                path: tmp_path.clone(),
                source: e,
            });
        }

        // Drop file handle before rename (required on Windows)
        drop(file);

        // Atomic rename
        if let Err(e) = fs::rename(&tmp_path, &self.target_path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(CueBladeError::Io {
                path: self.target_path.clone(),
                source: e,
            });
        }

        Ok(())
    }

    /// Generate a unique temporary file path in the target's parent directory.
    fn tmp_path(&self) -> PathBuf {
        let uuid = Uuid::new_v4();
        let name = format!(".cueblade-tmp-{uuid}");
        match self.target_path.parent() {
            Some(parent) => parent.join(name),
            None => PathBuf::from(name),
        }
    }

    /// Base directory for output operations.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Relative path within base directory.
    pub fn relative_path(&self) -> &Path {
        &self.relative_path
    }
}

/// Normalize a path by resolving `.` and `..` components lexically
/// (without touching the filesystem).
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            other => components.push(other),
        }
    }
    components.iter().collect()
}

/// Check that no component between base_dir and target is a symlink.
fn check_no_symlinks(target: &Path, base_dir: &Path) -> Result<()> {
    // Walk from base_dir toward target, checking each existing ancestor
    let relative = target
        .strip_prefix(base_dir)
        .map_err(|_| CueBladeError::InputValidation {
            reason: format!(
                "Cannot compute relative path from {} to {}",
                base_dir.display(),
                target.display()
            ),
        })?;

    let mut current = base_dir.to_path_buf();
    for component in relative.components() {
        current = current.join(component);
        // Only check if the path exists (intermediate dirs may not exist yet)
        if current.exists() || current.symlink_metadata().is_ok() {
            let meta = fs::symlink_metadata(&current).map_err(|e| CueBladeError::Io {
                path: current.clone(),
                source: e,
            })?;
            if meta.file_type().is_symlink() {
                return Err(CueBladeError::InputValidation {
                    reason: format!("Symlink detected in output path: {}", current.display()),
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_atomic_write_success() {
        let dir = tempfile::tempdir().unwrap();
        let writer = AtomicWriter::new(dir.path(), Path::new("test.txt")).unwrap();

        writer
            .write_with(|f| {
                f.write_all(b"hello world")?;
                Ok(())
            })
            .unwrap();

        let content = fs::read_to_string(dir.path().join("test.txt")).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_atomic_write_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let writer = AtomicWriter::new(dir.path(), Path::new("sub/dir/test.txt")).unwrap();

        writer
            .write_with(|f| {
                f.write_all(b"nested")?;
                Ok(())
            })
            .unwrap();

        assert!(dir.path().join("sub/dir/test.txt").exists());
    }

    #[test]
    fn test_atomic_write_cleanup_on_error() {
        let dir = tempfile::tempdir().unwrap();
        let writer = AtomicWriter::new(dir.path(), Path::new("fail.txt")).unwrap();

        let result = writer.write_with(|_f| Err(CueBladeError::Other("intentional error".into())));

        assert!(result.is_err());

        // No temp files should remain
        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with(".cueblade-tmp-"))
            })
            .collect();
        assert!(entries.is_empty());

        // Target file should not exist
        assert!(!dir.path().join("fail.txt").exists());
    }

    #[test]
    fn test_path_traversal_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let result = AtomicWriter::new(dir.path(), Path::new("../../etc/passwd"));
        assert!(matches!(result, Err(CueBladeError::InputValidation { .. })));
    }

    #[test]
    fn test_symlink_in_output_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let real_dir = dir.path().join("real");
        fs::create_dir(&real_dir).unwrap();

        let link = dir.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real_dir, &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&real_dir, &link).unwrap();

        let result = AtomicWriter::new(dir.path(), Path::new("link/test.txt"));
        assert!(matches!(result, Err(CueBladeError::InputValidation { .. })));
    }

    #[test]
    fn test_normalize_path() {
        assert_eq!(
            normalize_path(Path::new("/a/b/../c")),
            PathBuf::from("/a/c")
        );
        assert_eq!(
            normalize_path(Path::new("/a/./b/./c")),
            PathBuf::from("/a/b/c")
        );
        assert_eq!(
            normalize_path(Path::new("/a/b/c/../../d")),
            PathBuf::from("/a/d")
        );
    }
}
