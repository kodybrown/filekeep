use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tempfile::NamedTempFile;

/// Read fixed-size blocks instead of hashing or normalizing text. Equal means
/// equal bytes, including BOMs, trailing spaces, and line endings.
pub fn files_equal(left: &Path, right: &Path) -> Result<bool> {
    let left = File::open(left)?;
    let right = File::open(right)?;
    if left.metadata()?.len() != right.metadata()?.len() {
        return Ok(false);
    }
    let mut left = BufReader::new(left);
    let mut right = BufReader::new(right);
    readers_equal(&mut left, &mut right)
}

pub fn readers_equal(left: &mut dyn Read, right: &mut dyn Read) -> Result<bool> {
    let mut a = [0_u8; 64 * 1024];
    let mut b = [0_u8; 64 * 1024];
    loop {
        let a_size = read_block(left, &mut a)?;
        let b_size = read_block(right, &mut b)?;
        if a_size != b_size || a[..a_size] != b[..b_size] {
            return Ok(false);
        }
        if a_size == 0 {
            return Ok(true);
        }
    }
}

fn read_block(reader: &mut dyn Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut count = 0;
    while count < buffer.len() {
        match reader.read(&mut buffer[count..]) {
            Ok(0) => break,
            Ok(size) => count += size,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(count)
}

pub fn numbered_path(base: &Path, number: u64) -> Result<PathBuf> {
    let stem = base.file_stem().context("Output has no filename")?;
    let mut name = stem.to_os_string();
    name.push(format!("-{number:03}"));
    if let Some(extension) = base.extension() {
        name.push(".");
        name.push(extension);
    }
    Ok(base.with_file_name(name))
}

pub fn next_archive(base: &Path) -> Result<PathBuf> {
    let directory = base.parent().context("Output has no parent")?;
    let stem = base
        .file_stem()
        .and_then(|s| s.to_str())
        .context("Output filename is not UTF-8")?;
    let extension = base.extension().and_then(|s| s.to_str());
    let prefix = format!("{stem}-");
    let suffix = extension.map(|ext| format!(".{ext}")).unwrap_or_default();
    let mut maximum = 0_u64;
    for entry in fs::read_dir(directory)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        let name = if cfg!(windows) {
            name.to_lowercase()
        } else {
            name
        };
        let prefix = if cfg!(windows) {
            prefix.to_lowercase()
        } else {
            prefix.clone()
        };
        let suffix = if cfg!(windows) {
            suffix.to_lowercase()
        } else {
            suffix.clone()
        };
        if let Some(middle) = name
            .strip_prefix(&prefix)
            .and_then(|s| s.strip_suffix(&suffix))
            && middle.len() >= 3
            && middle.bytes().all(|b| b.is_ascii_digit())
        {
            maximum = maximum.max(
                middle
                    .parse::<u64>()
                    .context("Archive number is too large")?,
            );
        }
    }
    numbered_path(
        base,
        maximum
            .checked_add(1)
            .context("Archive numbering exhausted")?,
    )
}

/// Caller holds its destination lock. Preserve the old base before atomically
/// replacing it, so an interrupted publication never leaves the base missing.
/// Hard links avoid recopying large backups; exclusive copying also supports
/// filesystems without hard-link support. Neither route overwrites an archive.
pub fn publish_with_history(staged: NamedTempFile, base: &Path) -> Result<Option<PathBuf>> {
    staged.as_file().sync_all()?;
    let existing = match fs::symlink_metadata(base) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let archive = if let Some(metadata) = existing {
        if !metadata.file_type().is_file() {
            bail!("Existing output is not a regular file: {}", base.display());
        }
        let archive = next_archive(base)?;
        if let Err(link_error) = fs::hard_link(base, &archive) {
            if archive.try_exists()? {
                return Err(link_error).context("Archive already exists or cannot be inspected");
            }
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&archive)?;
            let copied = (|| -> Result<()> {
                io::copy(&mut File::open(base)?, &mut output)?;
                output.sync_all()?;
                Ok(())
            })();
            if let Err(error) = copied {
                drop(output);
                let _ = fs::remove_file(&archive);
                return Err(error).context("Cannot preserve previous backup");
            }
        }
        Some(archive)
    } else {
        None
    };
    staged
        .persist(base)
        .map_err(|error| error.error)
        .with_context(|| {
            format!(
                "Cannot publish {}; previous backup remains preserved",
                base.display()
            )
        })?;
    Ok(archive)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut staged = NamedTempFile::new_in(path.parent().context("File has no parent")?)?;
    staged.write_all(bytes)?;
    staged.as_file().sync_all()?;
    staged.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn publish(base: &Path, bytes: &[u8]) -> Option<PathBuf> {
        let mut staged = NamedTempFile::new_in(base.parent().unwrap()).unwrap();
        staged.write_all(bytes).unwrap();
        publish_with_history(staged, base).unwrap()
    }

    #[test]
    fn oldest_stays_001_newest_keeps_the_base_name() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("settings.tar.gz");
        assert!(publish(&base, b"first").is_none());
        publish(&base, b"second");
        publish(&base, b"third");
        assert_eq!(fs::read(&base).unwrap(), b"third");
        assert_eq!(
            fs::read(directory.path().join("settings.tar-001.gz")).unwrap(),
            b"first"
        );
        assert_eq!(
            fs::read(directory.path().join("settings.tar-002.gz")).unwrap(),
            b"second"
        );
        fs::remove_file(directory.path().join("settings.tar-001.gz")).unwrap();
        publish(&base, b"fourth");
        assert_eq!(
            fs::read(directory.path().join("settings.tar-003.gz")).unwrap(),
            b"third"
        );
    }

    #[test]
    fn byte_comparison_preserves_bom_whitespace_and_binary_data() {
        let directory = tempfile::tempdir().unwrap();
        let a = directory.path().join("a");
        let b = directory.path().join("b");
        for (left, right, equal) in [
            (&b"x\r\n"[..], &b"x\n"[..], false),
            (&b"x "[..], &b"x"[..], false),
            (&b"\xef\xbb\xbfx"[..], &b"x"[..], false),
            (&b"\0\xff\xfe"[..], &b"\0\xff\xfe"[..], true),
        ] {
            fs::write(&a, left).unwrap();
            fs::write(&b, right).unwrap();
            assert_eq!(files_equal(&a, &b).unwrap(), equal);
        }
    }

    #[test]
    fn numbering_handles_extensionless_names_and_more_than_999_versions() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("hosts");
        fs::write(directory.path().join("hosts-999"), "old").unwrap();
        assert_eq!(
            next_archive(&base).unwrap(),
            directory.path().join("hosts-1000")
        );
        assert_eq!(
            numbered_path(&directory.path().join(".env"), 1).unwrap(),
            directory.path().join(".env-001")
        );
    }

    #[test]
    fn comparison_handles_short_reads_and_changes_beyond_a_block() {
        struct ShortReader<'a>(&'a [u8]);
        impl Read for ShortReader<'_> {
            fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
                let count = output.len().min(17);
                self.0.read(&mut output[..count])
            }
        }
        let bytes = vec![0xab; 150_000];
        assert!(readers_equal(&mut ShortReader(&bytes), &mut &bytes[..]).unwrap());
        let mut changed = bytes.clone();
        changed[149_999] = 0xcd;
        assert!(!readers_equal(&mut ShortReader(&bytes), &mut &changed[..]).unwrap());
    }
}
