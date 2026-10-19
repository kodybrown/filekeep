use anyhow::{Context, Result};

use crate::config::EnvironmentScope;

pub(super) fn capture(scopes: &[EnvironmentScope]) -> Result<Vec<u8>> {
    let mut output = String::new();
    for (index, scope) in scopes.iter().enumerate() {
        // Collect the complete scope before writing its section. A registry or
        // decoding failure must not be published as a successful partial dump.
        let variables = read_scope(*scope)
            .with_context(|| format!("Cannot read {} environment variables", scope.label()))?;
        if index > 0 {
            output.push('\n');
        }
        output.push_str(&format!("# {} Environment\n", scope.label()));
        output.push_str(&render(variables));
    }
    Ok(output.into_bytes())
}

fn read_scope(scope: EnvironmentScope) -> Result<Vec<(String, String)>> {
    if scope == EnvironmentScope::Process {
        return std::env::vars_os()
            .map(|(key, value)| {
                let key = key
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("Environment variable name is not valid UTF-8"))?;
                let value = value.into_string().map_err(|_| {
                    anyhow::anyhow!("Environment variable {key:?} is not valid UTF-8")
                })?;
                Ok((key, value))
            })
            .collect();
    }
    read_persistent_scope(scope)
}

#[cfg(windows)]
fn read_persistent_scope(scope: EnvironmentScope) -> Result<Vec<(String, String)>> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::types::FromRegValue;

    let (hive, path) = match scope {
        EnvironmentScope::Machine => (
            HKEY_LOCAL_MACHINE,
            r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
        ),
        EnvironmentScope::User => (HKEY_CURRENT_USER, "Environment"),
        EnvironmentScope::Process => unreachable!("process scope handled separately"),
    };
    let key = RegKey::predef(hive).open_subkey(path)?;
    key.enum_values()
        .map(|item| {
            let (name, raw) = item?;
            // FromRegValue decodes REG_SZ/REG_EXPAND_SZ without expanding %VAR%.
            let value = String::from_reg_value(&raw)
                .with_context(|| format!("Cannot decode registry environment variable {name:?}"))?;
            Ok((name, value))
        })
        .collect()
}

#[cfg(not(windows))]
fn read_persistent_scope(_scope: EnvironmentScope) -> Result<Vec<(String, String)>> {
    anyhow::bail!(
        "machine/user environment sources require Windows; use scopes: [process] on this platform"
    )
}

fn render(mut variables: Vec<(String, String)>) -> String {
    variables.sort_by(|a, b| {
        a.0.to_lowercase()
            .cmp(&b.0.to_lowercase())
            .then_with(|| a.0.cmp(&b.0))
    });
    let mut output = String::new();
    for (name, value) in variables {
        if name.to_ascii_lowercase().contains("path") && value.contains(';') {
            output.push_str(&format!("{name} = (EXPANDED)\n"));
            for part in value
                .split(';')
                .map(str::trim)
                .filter(|part| !part.is_empty())
            {
                output.push_str(&format!("- {}\n", escape(part)));
            }
        } else {
            output.push_str(&format!("{name} = \"{}\"\n", escape(&value)));
        }
    }
    output
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_dump_is_stable_readable_and_keeps_raw_references() {
        let variables = vec![
            ("z".into(), "%HOME%\\folder\n\"x\"".into()),
            ("Path".into(), "C:\\one; ;%SystemRoot%\\two".into()),
            ("a".into(), "value".into()),
        ];
        let mut reversed = variables.clone();
        reversed.reverse();
        let output = render(variables);
        assert_eq!(render(reversed), output);
        assert!(output.starts_with("a = \"value\"\nPath = (EXPANDED)\n"));
        assert!(output.contains("%SystemRoot%"));
        assert!(output.contains("\\n\\\"x\\\""));
    }

    #[cfg(windows)]
    #[test]
    fn persistent_windows_scopes_are_readable_without_expanding_values() {
        let output = capture(&[EnvironmentScope::Machine, EnvironmentScope::User]).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.starts_with("# System Environment\n"));
        assert!(output.contains("\n# User Environment\n"));
    }
}
