use std::collections::HashMap;
use std::fmt::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, FixedOffset};

use crate::paths::home_directory;

/// Capture once so every entry in a run uses the same clock, host, and environment.
pub struct TemplateContext {
    pub time: DateTime<FixedOffset>,
    pub home: PathBuf,
    pub host: String,
    pub variables: HashMap<String, String>,
}

impl TemplateContext {
    pub fn capture() -> Result<Self> {
        let host = hostname::get()?
            .into_string()
            .map_err(|_| anyhow::anyhow!("Hostname is not valid UTF-8"))?;
        Ok(Self {
            time: chrono::Local::now().fixed_offset(),
            home: home_directory()?,
            host,
            variables: std::env::vars_os()
                .filter_map(|(key, value)| {
                    Some((key.into_string().ok()?, value.into_string().ok()?))
                })
                .collect(),
        })
    }

    pub fn expand(&self, input: &str) -> Result<String> {
        let mut value = input.to_owned();
        for _ in 0..32 {
            let (next, changed) = self.expand_once(&value)?;
            if !changed {
                return Ok(next);
            }
            if next == value {
                bail!("Cyclic template expansion in {input:?}");
            }
            value = next;
        }
        bail!("Template expansion exceeded 32 levels in {input:?}")
    }

    pub fn expand_path(&self, input: &str) -> Result<PathBuf> {
        let expanded = self.expand(input)?;
        if expanded == "~" {
            return Ok(self.home.clone());
        }
        if let Some(rest) = expanded
            .strip_prefix("~/")
            .or_else(|| expanded.strip_prefix("~\\"))
        {
            return Ok(self.home.join(rest));
        }
        Ok(PathBuf::from(expanded))
    }

    fn expand_once(&self, input: &str) -> Result<(String, bool)> {
        let mut output = String::new();
        let mut offset = 0;
        let mut changed = false;
        while offset < input.len() {
            let rest = &input[offset..];
            let token = if rest.starts_with("${") {
                Some((2, '}'))
            } else if rest.starts_with("$(") {
                Some((2, ')'))
            } else if rest.starts_with('%') {
                Some((1, '%'))
            } else {
                None
            };
            if let Some((prefix, suffix)) = token {
                let end = rest[prefix..]
                    .find(suffix)
                    .context("Unclosed template token")?
                    + prefix;
                output.push_str(&self.resolve(&rest[prefix..end])?);
                offset += end + 1;
                changed = true;
            } else {
                let character = rest.chars().next().unwrap();
                output.push(character);
                offset += character.len_utf8();
            }
        }
        Ok((output, changed))
    }

    fn resolve(&self, token: &str) -> Result<String> {
        if token
            .get(..5)
            .is_some_and(|s| s.eq_ignore_ascii_case("date:"))
        {
            return format_date(&token[5..], self.time);
        }
        let token = if token
            .get(..4)
            .is_some_and(|s| s.eq_ignore_ascii_case("env:"))
        {
            &token[4..]
        } else {
            token
        };
        match token.to_ascii_uppercase().as_str() {
            "HOME" | "USERPROFILE" => {
                return Ok(self
                    .home
                    .to_str()
                    .context("Home path is not valid UTF-8")?
                    .to_owned());
            }
            "HOST" | "HOSTNAME" | "COMPUTERNAME" => return Ok(self.host.clone()),
            _ => {}
        }
        let value = if cfg!(windows) {
            self.variables
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(token))
                .map(|(_, value)| value)
        } else {
            self.variables.get(token)
        };
        value
            .cloned()
            .with_context(|| format!("Unknown environment variable or template token {token:?}"))
    }
}

/// Preserve the old utilities' common .NET-style date tokens without pretending
/// that Rust's strftime grammar is interchangeable with .NET's custom formats.
fn format_date(format: &str, time: DateTime<FixedOffset>) -> Result<String> {
    let mut result = String::new();
    let mut offset = 0;
    if format.is_empty() {
        bail!("Date format is empty");
    }
    const TOKENS: &[(&str, &str)] = &[
        ("yyyy", "%Y"),
        ("fff", "%3f"),
        ("yy", "%y"),
        ("MM", "%m"),
        ("dd", "%d"),
        ("HH", "%H"),
        ("mm", "%M"),
        ("ss", "%S"),
    ];
    while offset < format.len() {
        let rest = &format[offset..];
        if let Some((token, replacement)) = TOKENS.iter().find(|(token, _)| rest.starts_with(token))
        {
            write!(result, "{}", time.format(replacement))?;
            offset += token.len();
        } else {
            let character = rest.chars().next().unwrap();
            if character.is_alphabetic() || character == '%' {
                bail!(
                    "Unsupported date format near {rest:?}; supported tokens: yyyy, yy, MM, dd, HH, mm, ss, fff"
                );
            }
            result.push(character);
            offset += character.len_utf8();
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> TemplateContext {
        TemplateContext {
            time: DateTime::parse_from_rfc3339("2026-10-19T09:08:07.123-06:00").unwrap(),
            home: PathBuf::from("/home/test"),
            host: "MACHINE".into(),
            variables: [
                ("NESTED".into(), "${HOST}".into()),
                ("EMPTY".into(), "".into()),
                ("CYCLE".into(), "${CYCLE}".into()),
            ]
            .into(),
        }
    }

    #[test]
    fn expands_old_syntax_and_one_consistent_date() {
        let context = context();
        assert_eq!(
            context
                .expand("%HOST%-$(HOSTNAME)-${COMPUTERNAME}-${env:NESTED}")
                .unwrap(),
            "MACHINE-MACHINE-MACHINE-MACHINE"
        );
        assert_eq!(
            context
                .expand("${date:yyyyMMdd-HHmmss-fff}-%date:yyMM% -$(date:dd)")
                .unwrap(),
            "20261019-090807-123-2610 -19"
        );
        assert_eq!(context.expand("${EMPTY}").unwrap(), "");
        assert_eq!(
            context.expand_path("~/settings").unwrap(),
            context.home.join("settings")
        );
    }

    #[test]
    fn unresolved_cycles_and_bad_dates_fail() {
        for input in ["${MISSING}", "${CYCLE}", "${HOST", "${date:YYYY}"] {
            assert!(context().expand(input).is_err(), "{input}");
        }
    }
}
