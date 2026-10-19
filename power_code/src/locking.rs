//! Destination locking with no persistent marker after the owning process exits.

use anyhow::{Context, Result};
use std::{fs::File, path::Path};

pub struct DirectoryLock {
    // Keep the handle alive through every destination mutation. No explicit
    // unlock/delete gap is introduced when this guard drops or the process dies.
    _file: File,
}

impl DirectoryLock {
    pub fn try_acquire(directory: &Path) -> Result<Self> {
        acquire(directory)
            .map(|file| Self { _file: file })
            .context("Destination is in use by another filekeep process or cannot be locked")
    }
}

#[cfg(windows)]
fn acquire(directory: &Path) -> Result<File> {
    use anyhow::ensure;
    use std::{
        fs::{self, OpenOptions},
        os::windows::fs::OpenOptionsExt,
    };
    use windows_sys::Win32::{
        Foundation::{GENERIC_READ, GENERIC_WRITE},
        Storage::FileSystem::{DELETE, FILE_FLAG_DELETE_ON_CLOSE, FILE_FLAG_OPEN_REPARSE_POINT},
    };
    let path = directory.join("lock");
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        ensure!(
            metadata.file_type().is_file() && !metadata.file_type().is_symlink(),
            "Lock path is not a regular file"
        );
    }
    // Exclusive sharing keeps other processes from opening/removing this marker
    // until Windows deletes it as part of closing the final owning handle.
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .access_mode(GENERIC_READ | GENERIC_WRITE | DELETE)
        .share_mode(0)
        .custom_flags(FILE_FLAG_DELETE_ON_CLOSE | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?)
}

#[cfg(unix)]
fn acquire(directory: &Path) -> Result<File> {
    use fs2::FileExt;
    // Directory inodes already have a stable lifetime, so Unix needs no marker.
    let file = File::open(directory)?;
    file.try_lock_exclusive()?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contention_and_cleanup_on_success_or_error() {
        let directory = tempfile::tempdir().unwrap();
        // A marker left by a previous version is removed by the same handle
        // lifetime as a newly created Windows marker.
        #[cfg(windows)]
        std::fs::write(directory.path().join("lock"), "").unwrap();
        let first = DirectoryLock::try_acquire(directory.path()).unwrap();
        assert!(DirectoryLock::try_acquire(directory.path()).is_err());
        drop(first);
        assert!(!directory.path().join("lock").exists());
        let fail = || -> Result<()> {
            let _lock = DirectoryLock::try_acquire(directory.path())?;
            anyhow::bail!("simulated failure")
        };
        assert!(fail().is_err());
        assert!(!directory.path().join("lock").exists());
        assert!(DirectoryLock::try_acquire(directory.path()).is_ok());
    }
}
