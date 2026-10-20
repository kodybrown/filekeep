// dotask: 1
// description: Validate and exercise a Windows installer in temporary locations without changing PATH.
// options:
//   - {name: installer, type: path, required: true, description: Self-extracting installer produced by create-filekeep-installer.}
// end-dotask
use dotask_sdk::{BuildContext, Context, Result, Value, serde_json, serde_saphyr, tempfile};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    dotask_sdk::run(task);
}

fn run(executable: &Path, arguments: &[String], cwd: &Path, config_home: &Path) -> Result<String> {
    let result = Command::new(executable)
        .args(arguments)
        .current_dir(cwd)
        .env("APPDATA", config_home)
        // Keep the obsolete Windows XDG location separate: acceptance must prove
        // that both installation and application discovery use roaming AppData.
        .env("XDG_CONFIG_HOME", config_home.join("unused-xdg"))
        .output()?;
    if !result.status.success() {
        dotask_sdk::bail!(
            "Acceptance command failed: {}\n{}\n{}",
            executable.display(),
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    Ok(String::from_utf8(result.stdout)?)
}

fn paths() -> Result<Vec<(bool, Vec<u8>)>> {
    [
        ("HKCU\\Environment", "Path"),
        (
            "HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
            "Path",
        ),
    ]
    .into_iter()
    .map(|(key, value)| {
        let output = Command::new("reg.exe")
            .args(["query", key, "/v", value])
            .output()?;
        Ok((output.status.success(), output.stdout))
    })
    .collect()
}

fn task(project: &BuildContext) -> Result<()> {
    if !cfg!(windows) {
        dotask_sdk::bail!("This native acceptance task currently covers Windows only.");
    }
    let before = paths()?;
    let temp = tempfile::Builder::new()
        .prefix("filekeep installer acceptance ")
        .tempdir()?;
    let cwd = temp.path().join("working directory");
    fs::create_dir(&cwd)?;
    let root = temp.path().join("installed application");
    let config_home = temp.path().join("config");
    let settings = config_home.join("filekeep/settings.yaml");
    let backups = temp.path().join("backups");
    let sources = temp.path().join("retained sources");
    let installation = vec![
        "--non-interactive".into(),
        "--install-dir".into(),
        root.to_string_lossy().into_owned(),
        "--set".into(),
        "add-to-path=false".into(),
        "--set".into(),
        "additional-command=false".into(),
        "--set".into(),
        format!("hosts-output-directory={}", backups.join("hosts").display()),
        "--set".into(),
        "hosts-filename=HOST-${date:yyyyMMdd}.txt".into(),
        "--set".into(),
        format!(
            "environment-output-directory={}",
            backups.join("environment").display()
        ),
        "--set".into(),
        "environment-filename=ENV-${date:yyyyMMdd}.txt".into(),
    ];
    let mut validation = vec![
        "--extract-to".into(),
        sources.to_string_lossy().into_owned(),
        "--".into(),
        "--validate".into(),
    ];
    validation.extend(installation.clone());
    run(
        &project.path(project.string("installer")?),
        &validation,
        &cwd,
        &config_home,
    )?;
    if root.exists() || settings.exists() || backups.exists() {
        dotask_sdk::bail!("Validation wrote application/settings/backup destinations");
    }
    let installers: Vec<PathBuf> = fs::read_dir(&sources)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|path| path.extension().is_some_and(|extension| extension == "exe"))
        .collect();
    if installers.len() != 1 {
        dotask_sdk::bail!("Expected one extracted installer executable");
    }
    let installer = &installers[0];
    run(installer, &installation, &cwd, &config_home)?;
    let contents = fs::read_to_string(&settings)?;
    let config: Value = serde_saphyr::from_str(&contents)?;
    if config["entries"][0]["id"] != "hosts"
        || config["entries"][0]["source"]["type"] != "hosts_file"
        || config["entries"][1]["id"] != "environment"
        || config["entries"][1]["source"]["type"] != "environment"
        || config["entries"][0]["filename"] != "HOST-${date:yyyyMMdd}.txt"
        || config["entries"][1]["filename"] != "ENV-${date:yyyyMMdd}.txt"
        || config["entries"][0]["skip_if_unchanged"] != true
        || config["entries"][1]["skip_if_unchanged"] != true
    {
        dotask_sdk::bail!(
            "Installed settings do not preserve filename templates and duplicate defaults"
        );
    }
    if backups.exists() {
        dotask_sdk::bail!("Installation ran backups");
    }
    let executable = root.join("filekeep.exe");
    let preview = run(&executable, &["--dry-run".into()], &cwd, &config_home)?;
    if !preview.contains("2 planned, 0 failed") || backups.exists() {
        dotask_sdk::bail!("Installed command did not discover/preview both sources");
    }
    run(&executable, &[], &cwd, &config_home)?;
    let repeated = run(&executable, &[], &cwd, &config_home)?;
    if !repeated.contains("0 created, 2 skipped") {
        dotask_sdk::bail!("Repeated backup did not skip exact matches");
    }
    for directory in ["hosts", "environment"] {
        if backups.join(directory).join(".filekeep/lock").exists() {
            dotask_sdk::bail!("A lock marker remained after backup");
        }
    }
    let customized = format!("{contents}\n# user-maintained configuration\r\n");
    fs::write(&settings, &customized)?;
    let mut changed = installation.clone();
    let name = changed
        .iter_mut()
        .find(|value| value.starts_with("hosts-filename="))
        .context("Missing fixture input")?;
    *name = "hosts-filename=do-not-overwrite.txt".into();
    run(installer, &changed, &cwd, &config_home)?;
    if fs::read(&settings)? != customized.as_bytes() {
        dotask_sdk::bail!("Reinstall rewrote customized settings");
    }
    // Reuse the verified payload under a distinct package identity to exercise
    // upgrade activation without building or installing another application.
    let source_config = installer.with_file_name(format!(
        "{}.yaml",
        installer
            .file_stem()
            .context("Installer filename missing")?
            .to_string_lossy()
    ));
    let mut package: Value = serde_saphyr::from_str(&fs::read_to_string(&source_config)?)?;
    package["application"]["version"] = serde_json::json!("0.1.0-acceptance-upgrade");
    let build = package["application"]["build"]["name"]
        .as_str()
        .context("Package build identity missing")?
        .to_owned();
    package["application"]["build"]["name"] =
        serde_json::json!(format!("{build}-acceptance-upgrade"));
    fs::write(&source_config, serde_saphyr::to_string(&package)?)?;
    run(installer, &changed, &cwd, &config_home)?;
    if fs::read(&settings)? != customized.as_bytes() {
        dotask_sdk::bail!("Upgrade rewrote customized settings");
    }
    run(
        &root.join("installer/installer.exe"),
        &[
            "uninstall".into(),
            "--non-interactive".into(),
            "--leave-settings".into(),
        ],
        &cwd,
        &config_home,
    )?;
    if fs::read(&settings)? != customized.as_bytes()
        || !backups.join("hosts").is_dir()
        || !backups.join("environment").is_dir()
    {
        dotask_sdk::bail!("Uninstall did not preserve settings and backups");
    }
    if config_home.join("unused-xdg").exists() {
        dotask_sdk::bail!("Windows installation or discovery used XDG_CONFIG_HOME");
    }
    if paths()? != before {
        dotask_sdk::bail!("Persistent user/machine PATH changed during acceptance");
    }
    println!(
        "Passed: self-extraction, validation, custom outputs, installed discovery, exact-match skips, lock cleanup, reinstall, upgrade, retained uninstall, and unchanged persistent PATH."
    );
    Ok(())
}
