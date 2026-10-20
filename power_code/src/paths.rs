use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

pub fn home_directory() -> Result<PathBuf> {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let home = std::env::var_os(key)
        .filter(|v| !v.is_empty())
        .context("Cannot locate home directory")?;
    let home = PathBuf::from(home);
    if !home.is_absolute() {
        bail!("Home directory must be absolute: {}", home.display());
    }
    Ok(home)
}

/// Use roaming AppData on Windows and XDG configuration on Linux/macOS.
/// Ignore the other platform's override, and reject empty/relative roots so
/// discovery cannot accidentally depend on the current working directory.
pub fn configuration_directory() -> Result<PathBuf> {
    let (key, fallback) = if cfg!(windows) {
        ("APPDATA", "AppData/Roaming")
    } else {
        ("XDG_CONFIG_HOME", ".config")
    };
    match std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
    {
        Some(path) => Ok(path),
        None => Ok(home_directory()?.join(fallback)),
    }
}

/// Lexical normalization works for destinations that do not exist yet.
pub fn absolute_from(path: &Path, base: &Path) -> Result<PathBuf> {
    // C:relative and \root-relative depend on hidden per-drive/CWD state on
    // Windows. Require an absolute path or an ordinary config-relative path.
    if !path.is_absolute()
        && path
            .components()
            .any(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
    {
        bail!(
            "Path must be absolute or relative to the settings directory: {}",
            path.display()
        );
    }
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    let path = std::path::absolute(path)?;
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                clean.pop();
            }
            other => clean.push(other.as_os_str()),
        }
    }
    Ok(clean)
}

/// Resolve existing ancestors as well, so aliases to the same directory compare equally.
pub fn resolved_path(path: &Path) -> Result<PathBuf> {
    let mut ancestor = path;
    let mut missing = Vec::new();
    loop {
        match ancestor.try_exists() {
            Ok(true) => break,
            Ok(false) => {
                missing.push(
                    ancestor
                        .file_name()
                        .context("Path has no existing ancestor")?
                        .to_os_string(),
                );
                ancestor = ancestor.parent().context("Path has no parent")?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    let mut result = ancestor.canonicalize()?;
    for component in missing.into_iter().rev() {
        result.push(component);
    }
    Ok(result)
}

pub fn same_path(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
    } else {
        left == right
    }
}

/// Require a portable basename; never silently turn a bad template into a different path.
pub fn validate_filename(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
    {
        bail!("Invalid output filename: {name:?}");
    }
    let device = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (device.len() == 4
            && (device.starts_with("COM") || device.starts_with("LPT"))
            && matches!(device.as_bytes()[3], b'1'..=b'9'))
    {
        bail!("Reserved output filename: {name:?}");
    }
    Ok(())
}

pub fn paths_alias(left: &Path, right: &Path) -> Result<bool> {
    if same_path(left, right) {
        return Ok(true);
    }
    if left.try_exists()? && right.try_exists()? {
        return Ok(same_file::is_same_file(left, right)?);
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_relative_to_the_supplied_base_and_names_are_portable() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            absolute_from(Path::new("one/../two"), root.path()).unwrap(),
            root.path().join("two")
        );
        for name in [
            "",
            "..",
            "../escape",
            "NUL.txt",
            "COM1",
            "bad:name",
            "trailing.",
        ] {
            assert!(validate_filename(name).is_err(), "{name}");
        }
        #[cfg(windows)]
        for path in [r"C:relative", r"\root-relative"] {
            assert!(absolute_from(Path::new(path), root.path()).is_err());
        }
    }
}
