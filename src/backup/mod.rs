mod state;

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use power_code::file_io::{files_equal, next_archive, publish_with_history, readers_equal};
use power_code::paths::{paths_alias, resolved_path};
use power_code::templates::TemplateContext;
use tempfile::NamedTempFile;

use crate::config::{PreparedEntry, Settings, prepare};
use crate::sources;

#[derive(Debug)]
pub enum Outcome {
    Created {
        output: PathBuf,
        archived: Option<PathBuf>,
    },
    Skipped {
        previous: PathBuf,
    },
    Planned {
        output: PathBuf,
        archive: Option<PathBuf>,
    },
}

#[derive(Debug)]
pub struct EntryReport {
    pub id: String,
    pub result: Result<Outcome>,
}

pub fn run(
    settings: &Settings,
    config_file: &Path,
    context: &TemplateContext,
    dry_run: bool,
) -> Vec<EntryReport> {
    let mut prepared: Vec<Result<PreparedEntry>> = settings
        .entries
        .iter()
        .map(|entry| prepare(entry, config_file, context))
        .collect();
    let mut conflicts: Vec<Option<String>> = (0..prepared.len()).map(|_| None).collect();
    for (index, entry) in settings.entries.iter().enumerate() {
        if settings
            .entries
            .iter()
            .filter(|other| other.id == entry.id)
            .count()
            > 1
        {
            conflicts[index] = Some(format!("Duplicate entry ID {:?}", entry.id));
        }
    }
    // Check the entire plan before writing. One entry must never overwrite
    // another's source, output, or this run's settings file.
    for (index, item) in prepared.iter().enumerate() {
        let Ok(entry) = item else { continue };
        let output = match resolved_path(&entry.output) {
            Ok(output) => output,
            Err(error) => {
                conflicts[index] = Some(format!("Cannot resolve output: {error:#}"));
                continue;
            }
        };
        let check = (|| -> Result<()> {
            if paths_alias(&output, config_file)? {
                bail!("Output would overwrite the configuration file");
            }
            for (other_index, other) in prepared.iter().enumerate() {
                let Ok(other) = other else { continue };
                if index != other_index && paths_alias(&entry.output, &other.output)? {
                    bail!("Output conflicts with entry {:?}", other.id);
                }
                if let Some(source) = other.source.file_path()
                    && paths_alias(&output, source)?
                {
                    bail!("Output would overwrite the source for entry {:?}", other.id);
                }
            }
            Ok(())
        })();
        if let Err(error) = check {
            conflicts[index] = Some(format!("{error:#}"));
        }
    }
    for (index, conflict) in conflicts.into_iter().enumerate() {
        if let Some(message) = conflict {
            prepared[index] = Err(anyhow::anyhow!(message));
        }
    }
    prepared
        .into_iter()
        .zip(&settings.entries)
        .map(|(entry, original)| {
            let result = entry.and_then(|entry| execute(&entry, config_file, context, dry_run));
            EntryReport {
                id: original.id.clone(),
                result,
            }
        })
        .collect()
}

fn execute(
    entry: &PreparedEntry,
    config_file: &Path,
    context: &TemplateContext,
    dry_run: bool,
) -> Result<Outcome> {
    let state_directory = entry.destination.join(".filekeep");
    let state_file = state::path(&state_directory, config_file, &entry.id, &context.host);
    if dry_run {
        let previous = state::read(&state_file, &entry.destination)?;
        let mut source = sources::open(&entry.source)?;
        if let Some(previous) = previous
            && entry.skip_if_unchanged
            && readers_equal(&mut source, &mut File::open(&previous)?)?
        {
            return Ok(Outcome::Skipped { previous });
        }
        // Read the entire source even when there is no comparison, so dry-run
        // catches read/registry failures without creating directories or files.
        io::copy(&mut source, &mut io::sink())?;
        let archive = planned_archive(&entry.output)?;
        return Ok(Outcome::Planned {
            output: entry.output.clone(),
            archive,
        });
    }

    fs::create_dir_all(&state_directory)
        .with_context(|| format!("Cannot create destination {}", entry.destination.display()))?;
    // Lock one destination across all configs/entries. A persistent lock file
    // avoids the inode race caused by deleting a lock file after unlocking it.
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(state_directory.join("lock"))?;
    lock.try_lock_exclusive()
        .context("Destination is in use by another filekeep process; retry after it finishes")?;
    let previous = state::read(&state_file, &entry.destination)?;
    let mut source = sources::open(&entry.source)?;
    let mut staged = NamedTempFile::new_in(&entry.destination)?;
    io::copy(&mut source, staged.as_file_mut())
        .context("Cannot capture source into a staged backup")?;
    if let Some(previous) = previous
        && entry.skip_if_unchanged
        && files_equal(staged.path(), &previous)?
    {
        return Ok(Outcome::Skipped { previous });
    }
    let archived = publish_with_history(staged, &entry.output)?;
    state::write(&state_file, &entry.output)?;
    Ok(Outcome::Created {
        output: entry.output.clone(),
        archived,
    })
}

fn planned_archive(output: &Path) -> Result<Option<PathBuf>> {
    match fs::symlink_metadata(output) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(Some(next_archive(output)?)),
        Ok(_) => bail!(
            "Existing output is not a regular file: {}",
            output.display()
        ),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
