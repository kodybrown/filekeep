use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use power_code::file_io::atomic_write;
use power_code::paths::validate_filename;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u32,
    latest: String,
}

pub fn path(directory: &Path, config_file: &Path, entry_id: &str, machine: &str) -> PathBuf {
    let mut digest = Sha256::new();
    // Length prefixes make the identity unambiguous. Config paths isolate jobs
    // using the same executable and entry IDs with different settings files.
    let config = config_file.to_string_lossy();
    let config = if cfg!(windows) {
        config.to_lowercase()
    } else {
        config.into_owned()
    };
    for part in [config.as_str(), entry_id, machine] {
        digest.update((part.len() as u64).to_le_bytes());
        digest.update(part.as_bytes());
    }
    directory.join(format!("{:x}.json", digest.finalize()))
}

pub fn read(path: &Path, destination: &Path) -> Result<Option<PathBuf>> {
    if !path.try_exists()? {
        return Ok(None);
    }
    let state: State = serde_json::from_slice(&std::fs::read(path)?)
        .with_context(|| format!("Cannot read backup state {}", path.display()))?;
    if state.version != 1 {
        bail!(
            "Unsupported backup state version {} in {}",
            state.version,
            path.display()
        );
    }
    validate_filename(&state.latest)?;
    let latest = destination.join(state.latest);
    match std::fs::symlink_metadata(&latest) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(Some(latest)),
        Ok(_) => bail!(
            "Previous backup is not a regular file: {}",
            latest.display()
        ),
        // A manually removed latest backup should be recreated, not skipped.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn write(path: &Path, output: &Path) -> Result<()> {
    let latest = output
        .file_name()
        .and_then(|s| s.to_str())
        .context("Output filename is not UTF-8")?
        .to_owned();
    let state = State { version: 1, latest };
    atomic_write(path, &serde_json::to_vec_pretty(&state)?).with_context(|| {
        format!(
            "Backup was published but its state could not be saved to {}",
            path.display()
        )
    })
}
