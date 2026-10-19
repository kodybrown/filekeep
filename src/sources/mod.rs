mod environment;

use std::fs::File;
use std::io::{Cursor, Read};

use anyhow::{Context, Result};

use crate::config::PreparedSource;

/// File reads stream into the staged backup. Generated sources use the same
/// pipeline and never print environment values to the console.
pub fn open(source: &PreparedSource) -> Result<Box<dyn Read>> {
    match source {
        PreparedSource::File(path) => {
            Ok(Box::new(File::open(path).with_context(|| {
                format!("Cannot read source {}", path.display())
            })?))
        }
        PreparedSource::Environment(scopes) => {
            Ok(Box::new(Cursor::new(environment::capture(scopes)?)))
        }
    }
}
