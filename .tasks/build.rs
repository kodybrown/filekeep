// dotask: 1
// description: Verify and build filekeep, optionally staging its installer payload.
// options:
//   - {name: stage-dir, type: path, description: 'Directory for the executable, defaults, and license.'}
// requires: [{kind: tool, value: cargo}]
// end-dotask
use dotask_sdk::{BuildContext, Context, Result, serde_json};
use std::{fs, path::PathBuf};

fn main() {
    dotask_sdk::run(task);
}

fn task(project: &BuildContext) -> Result<()> {
    let mut format_tasks = project.command("rustfmt");
    format_tasks.args(["--check", "--edition", "2024"]);
    for task in [
        "build.rs",
        "create-filekeep-installer.rs",
        "verify-installer.rs",
    ] {
        format_tasks.arg(project.path(format!(".tasks/{task}")));
    }
    project.execute(&mut format_tasks)?;
    // Build and verify the same native Windows payload without redistributable
    // requirements. Preserve existing caller flags, scoped to child commands.
    for arguments in [
        vec!["fmt", "--all", "--", "--check"],
        vec![
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
        vec!["test", "--workspace", "--locked"],
        vec!["build", "--release", "--locked"],
    ] {
        let mut command = project.command("cargo");
        command.args(arguments);
        if cfg!(windows) {
            command.env(
                "RUSTFLAGS",
                format!(
                    "{} -C target-feature=+crt-static",
                    std::env::var("RUSTFLAGS").unwrap_or_default()
                ),
            );
        }
        project.execute(&mut command)?;
    }
    let metadata: serde_json::Value =
        serde_json::from_str(&project.capture(project.command("cargo").args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--locked",
        ]))?)?;
    let target = PathBuf::from(
        metadata["target_directory"]
            .as_str()
            .context("Cargo returned no target_directory")?,
    );
    let executable = if cfg!(windows) {
        "filekeep.exe"
    } else {
        "filekeep"
    };
    let output = target.join("release").join(executable);
    if let Ok(stage) = project.string("stage-dir") {
        let stage = project.path(stage);
        fs::create_dir_all(stage.join("defaults"))?;
        fs::copy(&output, stage.join(executable))?;
        // Keep defaults away from the executable, so config discovery always
        // reaches the user's mutable settings rather than an immutable example.
        fs::copy(
            project.path("filekeep.yaml"),
            stage.join("defaults/filekeep.yaml"),
        )?;
        fs::copy(project.path("LICENSE.txt"), stage.join("LICENSE.txt"))?;
    }
    println!("{}", output.display());
    Ok(())
}
