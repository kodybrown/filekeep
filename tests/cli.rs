use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::json;

fn command(cwd: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_filekeep"));
    command.current_dir(cwd);
    command
}

fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn settings(source: &str, destination: &str, filename: &str) -> serde_json::Value {
    json!({ "entries": [{ "id": "one", "source": { "type": "file", "path": source },
        "destination": destination, "filename": filename }] })
}

#[test]
fn explicit_config_works_from_another_cwd_and_reports_partial_failures() {
    let directory = tempfile::tempdir().unwrap();
    let other_cwd = directory.path().join("other");
    fs::create_dir(&other_cwd).unwrap();
    fs::write(
        directory.path().join("input.bin"),
        b"\0\xff\xef\xbb\xbf\r\n",
    )
    .unwrap();
    let mut config = settings("input.bin", "backups", "output.bin");
    config["entries"].as_array_mut().unwrap().insert(0, json!({"id":"missing", "source":{"type":"file","path":"not-here"}, "destination":"backups", "filename":"missing"}));
    let config_file = directory.path().join("job.json");
    fs::write(&config_file, serde_json::to_vec(&config).unwrap()).unwrap();
    let output = command(&other_cwd)
        .args(["--config-file"])
        .arg(&config_file)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("[missing] Error:"));
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("1 created, 0 skipped, 0 planned, 1 failed")
    );
    assert_eq!(
        fs::read(directory.path().join("backups/output.bin")).unwrap(),
        b"\0\xff\xef\xbb\xbf\r\n"
    );
    assert!(!other_cwd.join("backups").exists());
}

#[test]
fn cwd_discovery_and_explicit_missing_config_never_fall_back() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("input"), "bytes").unwrap();
    fs::write(
        directory.path().join("filekeep.json"),
        serde_json::to_vec(&settings("input", "backups", "latest")).unwrap(),
    )
    .unwrap();
    let output = success(command(directory.path()).arg("--dry-run").output().unwrap());
    assert!(output.contains("1 planned, 0 failed"));
    assert!(!directory.path().join("backups").exists());
    assert_eq!(
        command(directory.path())
            .args(["--config-file", "missing.json"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(1)
    );
    fs::write(directory.path().join("filekeep.toml"), "").unwrap();
    let output = command(directory.path()).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Multiple configuration files"));
}

#[test]
#[cfg(not(windows))]
fn xdg_discovery_uses_app_directory_and_invalid_relative_xdg_uses_home() {
    let directory = tempfile::tempdir().unwrap();
    let cwd = directory.path().join("cwd");
    fs::create_dir(&cwd).unwrap();
    let xdg = directory.path().join("xdg");
    let app = xdg.join("filekeep");
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("input"), "bytes").unwrap();
    let json = settings("input", "backups", "latest");
    // JSON is a YAML subset; use an unambiguous fixture for settings.yaml.
    fs::write(
        app.join("settings.yaml"),
        serde_json::to_vec(&json).unwrap(),
    )
    .unwrap();
    let output = success(
        command(&cwd)
            .env("XDG_CONFIG_HOME", &xdg)
            .env("APPDATA", directory.path().join("unused-appdata"))
            .arg("--dry-run")
            .output()
            .unwrap(),
    );
    assert!(output.contains("1 planned, 0 failed"));
    assert!(!app.join("backups").exists());

    let home = directory.path().join("home");
    let fallback = home.join(".config/filekeep");
    fs::create_dir_all(&fallback).unwrap();
    fs::write(fallback.join("input"), "bytes").unwrap();
    fs::write(
        fallback.join("settings.yaml"),
        serde_json::to_vec(&json).unwrap(),
    )
    .unwrap();
    let output = success(
        command(&cwd)
            .env("XDG_CONFIG_HOME", "relative/path")
            .env("HOME", &home)
            .arg("--dry-run")
            .output()
            .unwrap(),
    );
    assert!(output.contains("1 planned, 0 failed"));
}

#[test]
#[cfg(windows)]
fn windows_discovery_uses_roaming_appdata_and_ignores_xdg() {
    let directory = tempfile::tempdir().unwrap();
    let cwd = directory.path().join("cwd");
    let roaming = directory.path().join("roaming");
    let home = directory.path().join("home");
    let xdg = directory.path().join("xdg");
    fs::create_dir(&cwd).unwrap();
    // Invalid legacy settings must never be loaded, even when XDG is absolute.
    for root in [&xdg, &home.join(".config")] {
        let app = root.join("filekeep");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("settings.yaml"), "invalid: [").unwrap();
    }
    for root in [&roaming, &home.join("AppData/Roaming")] {
        let app = root.join("filekeep");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("input"), "bytes").unwrap();
        fs::write(
            app.join("settings.yaml"),
            serde_json::to_vec(&settings("input", "backups", "latest")).unwrap(),
        )
        .unwrap();
    }
    for override_value in [
        Some(roaming.as_os_str()),
        None,
        Some(std::ffi::OsStr::new("")),
        Some(std::ffi::OsStr::new("relative/path")),
    ] {
        let mut child = command(&cwd);
        child.env("USERPROFILE", &home).env("XDG_CONFIG_HOME", &xdg);
        if let Some(value) = override_value {
            child.env("APPDATA", value);
        } else {
            child.env_remove("APPDATA");
        }
        let output = success(child.arg("--dry-run").output().unwrap());
        let expected = if override_value == Some(roaming.as_os_str()) {
            roaming.join("filekeep/settings.yaml")
        } else {
            home.join("AppData/Roaming/filekeep/settings.yaml")
        }
        .canonicalize()
        .unwrap();
        assert!(output.contains(expected.to_str().unwrap()), "{output}");
        assert!(output.contains("1 planned, 0 failed"));
    }
    assert!(!roaming.join("filekeep/backups").exists());
    assert!(!home.join("AppData/Roaming/filekeep/backups").exists());
}

#[test]
fn same_executable_runs_multiple_configs_and_skips_only_its_own_previous_backup() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("input"), "bytes").unwrap();
    for job in ["a", "b"] {
        fs::write(
            directory.path().join(format!("{job}.json")),
            serde_json::to_vec(&settings("input", "backups", &format!("{job}.txt"))).unwrap(),
        )
        .unwrap();
        let output = success(
            command(directory.path())
                .args(["--config-file", &format!("{job}.json")])
                .output()
                .unwrap(),
        );
        assert!(output.contains("1 created"));
    }
    let output = success(
        command(directory.path())
            .args(["--config-file", "a.json"])
            .output()
            .unwrap(),
    );
    assert!(output.contains("0 created, 1 skipped"));
    assert!(directory.path().join("backups/a.txt").is_file());
    assert!(directory.path().join("backups/b.txt").is_file());
}

#[test]
fn process_environment_provider_and_cli_help_are_available() {
    let directory = tempfile::tempdir().unwrap();
    let config = json!({"entries":[{"id":"env", "source":{"type":"environment", "scopes":["process"]}, "destination":"backups", "filename":"environment.txt"}]});
    fs::write(
        directory.path().join("env.json"),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    success(
        command(directory.path())
            .env("FILEKEEP_TEST_RAW_VALUE", "%UNEXPANDED%")
            .args(["--config-file", "env.json"])
            .output()
            .unwrap(),
    );
    let dump = fs::read_to_string(directory.path().join("backups/environment.txt")).unwrap();
    assert!(dump.contains("FILEKEEP_TEST_RAW_VALUE = \"%UNEXPANDED%\""));
    let help = success(command(directory.path()).arg("--help").output().unwrap());
    assert!(help.contains("--config-file"));
    assert!(help.contains("--dry-run"));
    assert!(
        success(command(directory.path()).arg("--version").output().unwrap())
            .starts_with("filekeep ")
    );
}
