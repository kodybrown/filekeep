use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use power_code::paths::{absolute_from, resolved_path, validate_filename};
use power_code::templates::TemplateContext;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub entries: Vec<Entry>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub source: Source,
    pub destination: String,
    pub filename: String,
    #[serde(default = "default_true")]
    pub skip_if_unchanged: bool,
}

fn default_true() -> bool {
    true
}

// An explicit type field with ordinary fields gives XML the same schema as the
// other formats; format-specific enum tagging would make the XML shape differ.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Source {
    #[serde(rename = "type")]
    pub kind: SourceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<EnvironmentScope>>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    File,
    HostsFile,
    Environment,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(try_from = "String", into = "String")]
pub enum EnvironmentScope {
    Machine,
    User,
    Process,
}

// Deserialize scopes as scalar strings. XML otherwise interprets a sequence of
// enums using element names, unlike JSON/YAML/TOML's sequence of scalar values.
impl TryFrom<String> for EnvironmentScope {
    type Error = String;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        match value.as_str() {
            "machine" => Ok(Self::Machine),
            "user" => Ok(Self::User),
            "process" => Ok(Self::Process),
            _ => Err(format!(
                "Unknown environment scope {value:?}; expected machine, user, or process"
            )),
        }
    }
}

impl From<EnvironmentScope> for String {
    fn from(scope: EnvironmentScope) -> Self {
        match scope {
            EnvironmentScope::Machine => "machine",
            EnvironmentScope::User => "user",
            EnvironmentScope::Process => "process",
        }
        .into()
    }
}

impl EnvironmentScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Machine => "System",
            Self::User => "User",
            Self::Process => "Process",
        }
    }
}

#[derive(Debug)]
pub struct PreparedEntry {
    pub id: String,
    pub source: PreparedSource,
    pub destination: PathBuf,
    pub output: PathBuf,
    pub skip_if_unchanged: bool,
}

#[derive(Debug)]
pub enum PreparedSource {
    File(PathBuf),
    Environment(Vec<EnvironmentScope>),
}

impl PreparedSource {
    pub fn file_path(&self) -> Option<&Path> {
        match self {
            Self::File(path) => Some(path),
            Self::Environment(_) => None,
        }
    }
}

pub fn prepare(
    entry: &Entry,
    config_file: &Path,
    context: &TemplateContext,
) -> Result<PreparedEntry> {
    if entry.id.trim().is_empty() {
        bail!("Entry ID is empty");
    }
    if entry.destination.trim().is_empty() {
        bail!("Destination is empty");
    }
    let base = config_file
        .parent()
        .context("Configuration has no directory")?;
    let destination = context.expand_path(&entry.destination)?;
    if destination.as_os_str().is_empty() {
        bail!("Expanded destination is empty");
    }
    let destination = absolute_from(&destination, base)?;
    let destination = resolved_path(&destination)?;
    let filename = context.expand(&entry.filename)?;
    validate_filename(&filename)?;
    if filename.eq_ignore_ascii_case(".filekeep") {
        bail!("The filename .filekeep is reserved for backup state");
    }
    let output = destination.join(filename);
    let source = match entry.source.kind {
        SourceKind::File | SourceKind::HostsFile => {
            if entry.source.scopes.is_some() {
                bail!("scopes is only valid for environment sources");
            }
            let path = if entry.source.kind == SourceKind::HostsFile {
                if entry.source.path.is_some() {
                    bail!(
                        "hosts_file locates the hosts file automatically; use type: file for a custom path"
                    );
                }
                hosts_path()?
            } else {
                let input = entry
                    .source
                    .path
                    .as_deref()
                    .context("File source requires path")?;
                if input.trim().is_empty() {
                    bail!("Source path is empty");
                }
                absolute_from(&context.expand_path(input)?, base)?
            };
            let path = path
                .canonicalize()
                .with_context(|| format!("Cannot locate source {}", path.display()))?;
            if !path.is_file() {
                bail!("Source is not a regular file: {}", path.display());
            }
            PreparedSource::File(path)
        }
        SourceKind::Environment => {
            if entry.source.path.is_some() {
                bail!("Environment sources do not accept path");
            }
            let scopes = entry
                .source
                .scopes
                .clone()
                .unwrap_or_else(|| vec![EnvironmentScope::Machine, EnvironmentScope::User]);
            if scopes.is_empty() {
                bail!("Environment scopes is empty");
            }
            let mut seen = HashSet::new();
            if scopes.iter().any(|scope| !seen.insert(*scope)) {
                bail!("Environment scopes contains duplicates");
            }
            PreparedSource::Environment(scopes)
        }
    };
    Ok(PreparedEntry {
        id: entry.id.clone(),
        source,
        destination,
        output,
        skip_if_unchanged: entry.skip_if_unchanged,
    })
}

fn hosts_path() -> Result<PathBuf> {
    if cfg!(windows) {
        let windows = std::env::var_os("SystemRoot").context("SystemRoot is not set")?;
        Ok(PathBuf::from(windows).join("System32/drivers/etc/hosts"))
    } else {
        Ok(PathBuf::from("/etc/hosts"))
    }
}
