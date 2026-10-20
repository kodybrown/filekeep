use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;

use crate::paths::{absolute_from, configuration_directory};

const EXTENSIONS: &[&str] = &["yaml", "yml", "json", "xml", "toml"];

pub fn load<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("Cannot read configuration {}", path.display()))?;
    // A UTF-8 BOM is common in settings produced by Windows editors.
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let result: Result<T> = match extension.as_str() {
        "yaml" | "yml" => serde_saphyr::from_str(text).map_err(Into::into),
        "json" => serde_json::from_str(text).map_err(Into::into),
        "xml" => quick_xml::de::from_str(text).map_err(Into::into),
        "toml" => toml::from_str(text).map_err(Into::into),
        _ => bail!(
            "Unsupported configuration extension {extension:?}; use yaml, yml, json, xml, or toml"
        ),
    };
    result.with_context(|| format!("Invalid configuration {}", path.display()))
}

pub fn discover(
    explicit: Option<&Path>,
    executable: &Path,
    cwd: &Path,
    application: &str,
) -> Result<PathBuf> {
    if let Some(path) = explicit {
        let path = absolute_from(path, cwd)?;
        if !path.is_file() {
            bail!("Configuration file not found: {}", path.display());
        }
        return Ok(path);
    }
    let stem = executable
        .file_stem()
        .context("Executable has no filename")?;
    let exe_dir = executable.parent().context("Executable has no directory")?;
    for directory in [cwd, exe_dir] {
        let mut candidates = Vec::new();
        for extension in EXTENSIONS {
            // Do not use with_extension on the stem: executable names can contain dots.
            let mut name = stem.to_os_string();
            name.push(format!(".{extension}"));
            let path = directory.join(name);
            if path.try_exists()? {
                candidates.push(path);
            }
        }
        match candidates.len() {
            0 => {}
            1 => return Ok(candidates.remove(0)),
            _ => bail!(
                "Multiple configuration files in {}: {}. Select one with --config-file",
                directory.display(),
                candidates
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
    let fallback = configuration_directory()?
        .join(application)
        .join("settings.yaml");
    if fallback.is_file() {
        return Ok(fallback);
    }
    bail!(
        "No configuration found in {}, {}, or {}. Use --config-file <path>",
        cwd.display(),
        exe_dir.display(),
        fallback.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_order_ambiguity_and_explicit_selection() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().join("cwd");
        let exe_dir = temp.path().join("bin");
        fs::create_dir_all(&cwd).unwrap();
        fs::create_dir_all(&exe_dir).unwrap();
        let exe = exe_dir.join("custom.tool.exe");
        let exe_config = exe_dir.join("custom.tool.toml");
        fs::write(&exe_config, "").unwrap();
        assert_eq!(discover(None, &exe, &cwd, "test").unwrap(), exe_config);
        let local = cwd.join("custom.tool.yaml");
        fs::write(&local, "").unwrap();
        assert_eq!(discover(None, &exe, &cwd, "test").unwrap(), local);
        fs::write(cwd.join("custom.tool.json"), "").unwrap();
        assert!(discover(None, &exe, &cwd, "test").is_err());
        assert_eq!(
            discover(Some(Path::new("custom.tool.yaml")), &exe, &cwd, "test").unwrap(),
            local
        );
        assert!(discover(Some(Path::new("missing.yaml")), &exe, &cwd, "test").is_err());
    }
}
