// dotask: 1
// description: Create one self-extracting filekeep installer using prebuilt doinstall tools.
// options:
//   - {name: doinstall-tools, type: path, description: 'Directory containing matching builder, runtime, and launcher tools.'}
//   - {name: app-version, description: Package version; defaults to the filekeep Cargo version.}
//   - {name: build-stamp, description: Optional reproducible UTC YYDDD-HHMM build stamp.}
// requires:
//   - {kind: task, value: build}
//   - {kind: task, value: _/doinstall/create-installer}
// end-dotask
use dotask_sdk::{BuildContext, Context, Result, Value, json, serde_json};
use std::path::PathBuf;

fn main() {
    dotask_sdk::run(task);
}

fn task(project: &BuildContext) -> Result<()> {
    let mut parameters = json!({});
    let tools = if let Ok(path) = project.string("doinstall-tools") {
        Some(project.path(path))
    } else if let Ok(source) = project.setting_path("doinstall.source") {
        let metadata: Value = serde_json::from_str(&project.capture(
            project.command("cargo").current_dir(&source).args([
                "metadata",
                "--format-version",
                "1",
                "--no-deps",
                "--locked",
            ]),
        )?)?;
        Some(
            PathBuf::from(
                metadata["target_directory"]
                    .as_str()
                    .context("Doinstall Cargo output is missing")?,
            )
            .join("release"),
        )
    } else {
        None
    };
    if let Some(tools) = tools {
        let extension = if cfg!(windows) { ".exe" } else { "" };
        for (parameter, name) in [
            ("builder", "doinstall-builder"),
            ("installer", "doinstall"),
            ("launcher-template", "doinstall-launcher"),
        ] {
            let path = tools.join(format!("{name}{extension}"));
            if !path.is_file() {
                dotask_sdk::bail!(
                    "Missing prebuilt tool {}. Build doinstall first, or pass --doinstall-tools.",
                    path.display()
                );
            }
            parameters[parameter] = json!(path);
        }
    }
    let version = if let Ok(version) = project.string("app-version") {
        version.to_owned()
    } else {
        let metadata: Value =
            serde_json::from_str(&project.capture(project.command("cargo").args([
                "metadata",
                "--format-version",
                "1",
                "--no-deps",
                "--locked",
            ]))?)?;
        metadata["packages"]
            .as_array()
            .context("Cargo packages are missing")?
            .iter()
            .find(|package| package["name"] == "filekeep")
            .and_then(|package| package["version"].as_str())
            .context("filekeep Cargo version is missing")?
            .to_owned()
    };
    parameters["app-version"] = json!(version);
    if let Ok(stamp) = project.string("build-stamp") {
        parameters["build-stamp"] = json!(stamp);
    }
    let artifact = project.create_installer("_/doinstall/create-installer", parameters)?;
    project.set_installer_result(&artifact)
}
