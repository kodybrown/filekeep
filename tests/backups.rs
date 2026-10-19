use std::fs;
use std::path::{Path, PathBuf};

use filekeep::backup::{Outcome, run};
use filekeep::config::{Entry, Settings, Source, SourceKind};
use power_code::templates::TemplateContext;

fn context() -> TemplateContext {
    TemplateContext {
        time: chrono::DateTime::parse_from_rfc3339("2026-10-19T10:00:00-06:00").unwrap(),
        home: PathBuf::from("/home/test"),
        host: "TESTHOST".into(),
        variables: Default::default(),
    }
}

fn entry(id: &str, source: &str, destination: &str, filename: &str) -> Entry {
    Entry {
        id: id.into(),
        source: Source {
            kind: SourceKind::File,
            path: Some(source.into()),
            scopes: None,
        },
        destination: destination.into(),
        filename: filename.into(),
        skip_if_unchanged: true,
    }
}

fn config_file(root: &Path) -> PathBuf {
    let path = root.join("settings.yaml");
    fs::write(&path, "fixture").unwrap();
    path.canonicalize().unwrap()
}

#[test]
fn rotation_exact_comparison_and_no_skip_policy_work_together() {
    let directory = tempfile::tempdir().unwrap();
    let config = config_file(directory.path());
    let source = directory.path().join("input");
    let output = directory.path().join("backups/current.txt");
    let mut settings = Settings {
        entries: vec![entry("one", "input", "backups", "current.txt")],
    };
    fs::write(&source, b"\xef\xbb\xbforiginal\r\n").unwrap();
    assert!(matches!(
        &run(&settings, &config, &context(), false)[0].result,
        Ok(Outcome::Created { archived: None, .. })
    ));
    assert!(matches!(
        &run(&settings, &config, &context(), false)[0].result,
        Ok(Outcome::Skipped { .. })
    ));
    fs::write(&source, b"\xef\xbb\xbforiginal\n").unwrap();
    assert!(matches!(
        &run(&settings, &config, &context(), false)[0].result,
        Ok(Outcome::Created {
            archived: Some(_),
            ..
        })
    ));
    fs::write(&source, b"third\0\xff").unwrap();
    run(&settings, &config, &context(), false)[0]
        .result
        .as_ref()
        .unwrap();
    assert_eq!(fs::read(&output).unwrap(), b"third\0\xff");
    assert_eq!(
        fs::read(output.with_file_name("current-001.txt")).unwrap(),
        b"\xef\xbb\xbforiginal\r\n"
    );
    assert_eq!(
        fs::read(output.with_file_name("current-002.txt")).unwrap(),
        b"\xef\xbb\xbforiginal\n"
    );
    settings.entries[0].skip_if_unchanged = false;
    assert!(matches!(
        &run(&settings, &config, &context(), false)[0].result,
        Ok(Outcome::Created { .. })
    ));
    assert_eq!(
        fs::read(output.with_file_name("current-003.txt")).unwrap(),
        b"third\0\xff"
    );
}

#[test]
fn changing_template_dates_still_compares_the_previous_successful_backup() {
    let directory = tempfile::tempdir().unwrap();
    let config = config_file(directory.path());
    fs::write(directory.path().join("input"), "same").unwrap();
    let settings = Settings {
        entries: vec![entry("one", "input", "backups", "${date:yyyyMMdd}.txt")],
    };
    run(&settings, &config, &context(), false)[0]
        .result
        .as_ref()
        .unwrap();
    let mut next_day = context();
    next_day.time += chrono::Duration::days(1);
    assert!(matches!(
        &run(&settings, &config, &next_day, false)[0].result,
        Ok(Outcome::Skipped { .. })
    ));
    assert!(!directory.path().join("backups/20261020.txt").exists());
    fs::write(directory.path().join("input"), "changed").unwrap();
    run(&settings, &config, &next_day, false)[0]
        .result
        .as_ref()
        .unwrap();
    assert_eq!(
        fs::read(directory.path().join("backups/20261020.txt")).unwrap(),
        b"changed"
    );
}

#[test]
fn failure_in_one_entry_does_not_prevent_later_entries() {
    let directory = tempfile::tempdir().unwrap();
    let config = config_file(directory.path());
    fs::write(directory.path().join("input"), "healthy").unwrap();
    fs::write(directory.path().join("not-a-directory"), "obstruction").unwrap();
    let settings = Settings {
        entries: vec![
            entry("missing", "missing", "backups", "missing"),
            entry("blocked", "input", "not-a-directory/child", "blocked"),
            entry("invalid", "input", "backups", "${UNKNOWN}"),
            entry("healthy", "input", "backups", "healthy"),
        ],
    };
    let reports = run(&settings, &config, &context(), false);
    assert!(reports[..3].iter().all(|report| report.result.is_err()));
    assert!(reports[3].result.is_ok());
    assert_eq!(
        fs::read(directory.path().join("backups/healthy")).unwrap(),
        b"healthy"
    );
}

#[test]
fn dry_run_does_not_create_destinations_or_state() {
    let directory = tempfile::tempdir().unwrap();
    let config = config_file(directory.path());
    fs::write(directory.path().join("input"), "healthy").unwrap();
    let settings = Settings {
        entries: vec![entry("one", "input", "backups", "current.txt")],
    };
    assert!(matches!(
        &run(&settings, &config, &context(), true)[0].result,
        Ok(Outcome::Planned { .. })
    ));
    assert!(!directory.path().join("backups").exists());
    run(&settings, &config, &context(), false)[0]
        .result
        .as_ref()
        .unwrap();
    assert!(matches!(
        &run(&settings, &config, &context(), true)[0].result,
        Ok(Outcome::Skipped { .. })
    ));
    fs::write(directory.path().join("input"), "changed").unwrap();
    assert!(matches!(
        &run(&settings, &config, &context(), true)[0].result,
        Ok(Outcome::Planned {
            archive: Some(_),
            ..
        })
    ));
    assert!(!directory.path().join("backups/current-001.txt").exists());
    assert_eq!(
        fs::read(directory.path().join("backups/current.txt")).unwrap(),
        b"healthy"
    );
}

#[test]
fn entries_and_configs_keep_separate_previous_backup_state() {
    let directory = tempfile::tempdir().unwrap();
    let config = config_file(directory.path());
    fs::write(directory.path().join("input"), "same").unwrap();
    let settings = Settings {
        entries: vec![
            entry("one", "input", "backups", "one.txt"),
            entry("two", "input", "backups", "two.txt"),
        ],
    };
    assert!(
        run(&settings, &config, &context(), false)
            .iter()
            .all(|report| matches!(report.result, Ok(Outcome::Created { .. })))
    );
    assert!(
        run(&settings, &config, &context(), false)
            .iter()
            .all(|report| matches!(report.result, Ok(Outcome::Skipped { .. })))
    );
    let other = directory.path().join("other.yaml");
    fs::write(&other, "other").unwrap();
    assert!(
        run(&settings, &other.canonicalize().unwrap(), &context(), false)
            .iter()
            .all(|report| matches!(report.result, Ok(Outcome::Created { .. })))
    );
}

#[test]
fn duplicate_ids_outputs_and_source_overwrites_are_reported_before_writing() {
    let directory = tempfile::tempdir().unwrap();
    let config = config_file(directory.path());
    fs::write(directory.path().join("input"), "original").unwrap();
    let settings = Settings {
        entries: vec![
            entry("duplicate", "input", "backups", "one"),
            entry("duplicate", "input", "backups", "two"),
            entry("same-output-one", "input", "backups", "same"),
            entry("same-output-two", "input", "backups", "same"),
            entry("self", "input", ".", "input"),
            entry("config", "input", ".", "settings.yaml"),
            entry("healthy", "input", "backups", "healthy"),
        ],
    };
    let reports = run(&settings, &config, &context(), false);
    assert!(reports[..6].iter().all(|report| report.result.is_err()));
    assert!(reports[6].result.is_ok());
    assert_eq!(
        fs::read(directory.path().join("input")).unwrap(),
        b"original"
    );
    assert_eq!(fs::read(&config).unwrap(), b"fixture");
}

#[test]
fn manually_removed_previous_backup_is_recreated() {
    let directory = tempfile::tempdir().unwrap();
    let config = config_file(directory.path());
    fs::write(directory.path().join("input"), "same").unwrap();
    let settings = Settings {
        entries: vec![entry("one", "input", "backups", "current")],
    };
    run(&settings, &config, &context(), false)[0]
        .result
        .as_ref()
        .unwrap();
    fs::remove_file(directory.path().join("backups/current")).unwrap();
    assert!(matches!(
        &run(&settings, &config, &context(), false)[0].result,
        Ok(Outcome::Created { .. })
    ));
}

#[test]
fn invalid_existing_state_is_preserved_and_blocks_only_its_entry() {
    let directory = tempfile::tempdir().unwrap();
    let config = config_file(directory.path());
    fs::write(directory.path().join("input"), "old").unwrap();
    let settings = Settings {
        entries: vec![entry("one", "input", "backups", "current")],
    };
    run(&settings, &config, &context(), false)[0]
        .result
        .as_ref()
        .unwrap();
    let state = fs::read_dir(directory.path().join("backups/.filekeep"))
        .unwrap()
        .map(|item| item.unwrap().path())
        .find(|path| path.extension().is_some_and(|ext| ext == "json"))
        .unwrap();
    fs::write(&state, "unknown state").unwrap();
    fs::write(directory.path().join("input"), "new").unwrap();
    assert!(
        run(&settings, &config, &context(), false)[0]
            .result
            .is_err()
    );
    assert_eq!(fs::read(&state).unwrap(), b"unknown state");
    assert_eq!(
        fs::read(directory.path().join("backups/current")).unwrap(),
        b"old"
    );
}

#[test]
fn another_process_destination_lock_prevents_backup_mutations() {
    use power_code::locking::DirectoryLock;
    let directory = tempfile::tempdir().unwrap();
    let config = config_file(directory.path());
    fs::write(directory.path().join("input"), "new").unwrap();
    let lock_dir = directory.path().join("backups/.filekeep");
    fs::create_dir_all(&lock_dir).unwrap();
    let lock = DirectoryLock::try_acquire(&lock_dir).unwrap();
    let settings = Settings {
        entries: vec![entry("one", "input", "backups", "current")],
    };
    assert!(
        run(&settings, &config, &context(), false)[0]
            .result
            .is_err()
    );
    assert!(!directory.path().join("backups/current").exists());
    drop(lock);
    assert!(run(&settings, &config, &context(), false)[0].result.is_ok());
}

#[test]
fn hard_link_alias_of_a_source_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let config = config_file(directory.path());
    let source = directory.path().join("input");
    fs::write(&source, "original").unwrap();
    let alias = directory.path().join("alias");
    fs::hard_link(&source, &alias).unwrap();
    let settings = Settings {
        entries: vec![entry("alias", "input", ".", "alias")],
    };
    assert!(
        run(&settings, &config, &context(), false)[0]
            .result
            .is_err()
    );
    assert_eq!(fs::read(&source).unwrap(), b"original");
    assert!(!directory.path().join("alias-001").exists());
}

#[test]
fn an_empty_expanded_destination_is_an_entry_error() {
    let directory = tempfile::tempdir().unwrap();
    let config = config_file(directory.path());
    fs::write(directory.path().join("input"), "original").unwrap();
    let mut context = context();
    context.variables.insert("EMPTY".into(), String::new());
    let settings = Settings {
        entries: vec![entry("empty", "input", "${EMPTY}", "backup")],
    };
    assert!(run(&settings, &config, &context, false)[0].result.is_err());
    assert!(!directory.path().join("backup").exists());
    assert!(!directory.path().join(".filekeep").exists());
}
