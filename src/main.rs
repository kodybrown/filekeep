use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use filekeep::backup::{self, Outcome};
use filekeep::config::Settings;
use power_code::config::{discover, load};
use power_code::templates::TemplateContext;

#[derive(Parser)]
#[command(
    name = "filekeep",
    version,
    about = "Keep versioned copies of files and environment settings"
)]
struct Arguments {
    /// Use exactly this YAML, JSON, XML, or TOML settings file
    #[arg(long, value_name = "PATH")]
    config_file: Option<PathBuf>,

    /// Read sources and report planned backups without writing anything
    #[arg(long)]
    dry_run: bool,
}

fn main() -> ExitCode {
    match execute(Arguments::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("Error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn execute(arguments: Arguments) -> Result<bool> {
    let executable = std::env::current_exe().context("Cannot locate executable")?;
    let cwd = std::env::current_dir()?;
    let config_file = discover(
        arguments.config_file.as_deref(),
        &executable,
        &cwd,
        "filekeep",
    )?
    .canonicalize()?;
    let settings: Settings = load(&config_file)?;
    anyhow::ensure!(
        !settings.entries.is_empty(),
        "Configuration contains no backup entries"
    );
    let context = TemplateContext::capture()?;
    println!("Configuration: {}", config_file.display());
    if arguments.dry_run {
        println!("Dry run: no files or directories will be written.");
    }
    let mut created = 0;
    let mut skipped = 0;
    let mut planned = 0;
    let mut failed = 0;
    for report in backup::run(&settings, &config_file, &context, arguments.dry_run) {
        match report.result {
            Ok(Outcome::Created { output, archived }) => {
                created += 1;
                if let Some(archived) = archived {
                    println!("[{}] Archived previous: {}", report.id, archived.display());
                }
                println!("[{}] Created: {}", report.id, output.display());
            }
            Ok(Outcome::Skipped { previous }) => {
                skipped += 1;
                println!(
                    "[{}] Unchanged; skipped (previous: {})",
                    report.id,
                    previous.display()
                );
            }
            Ok(Outcome::Planned { output, archive }) => {
                planned += 1;
                if let Some(archive) = archive {
                    println!(
                        "[{}] Would archive previous: {}",
                        report.id,
                        archive.display()
                    );
                }
                println!("[{}] Would create: {}", report.id, output.display());
            }
            Err(error) => {
                failed += 1;
                eprintln!("[{}] Error: {error:#}", report.id);
            }
        }
    }
    println!("Summary: {created} created, {skipped} skipped, {planned} planned, {failed} failed.");
    Ok(failed == 0)
}
