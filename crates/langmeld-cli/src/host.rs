use std::{
    path::Path,
    process::{Command, ExitCode, Output},
};

use serde_json::Value;

use crate::cli::HostArgs;

/// Runs the project's Rust CLI while leaving language-specific behavior in its host.
pub(crate) fn run_host(arguments: &HostArgs) -> ExitCode {
    match run_host_inner(arguments) {
        Ok(status) => status,
        Err(message) => {
            eprintln!("langmeld: {message}");
            ExitCode::from(2)
        }
    }
}

fn run_host_inner(arguments: &HostArgs) -> Result<ExitCode, String> {
    let manifest_path = arguments.project.canonicalize().map_err(|error| {
        format!(
            "cannot resolve project manifest '{}': {error}",
            arguments.project.display()
        )
    })?;
    let project_root = manifest_path
        .parent()
        .ok_or_else(|| "project manifest has no parent directory".to_owned())?;
    let cargo_manifest = project_root.join("Cargo.toml");
    if !cargo_manifest.is_file() {
        return Err(format!(
            "no Cargo.toml beside project manifest '{}'",
            manifest_path.display()
        ));
    }

    let metadata_output = Command::new("cargo")
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(&cargo_manifest)
        .current_dir(project_root)
        .output()
        .map_err(|error| format!("cannot start cargo metadata: {error}"))?;
    if !metadata_output.status.success() {
        print_child_output(&metadata_output);
        return Err("cargo metadata failed for this project".to_owned());
    }
    let metadata: Value = serde_json::from_slice(&metadata_output.stdout)
        .map_err(|error| format!("cargo metadata returned invalid JSON: {error}"))?;
    let cargo_manifest = cargo_manifest.canonicalize().map_err(|error| {
        format!(
            "cannot resolve Cargo manifest '{}': {error}",
            cargo_manifest.display()
        )
    })?;
    let package = metadata
        .get("packages")
        .and_then(Value::as_array)
        .and_then(|packages| {
            packages.iter().find(|package| {
                package
                    .get("manifest_path")
                    .and_then(Value::as_str)
                    .and_then(|path| Path::new(path).canonicalize().ok())
                    .as_deref()
                    == Some(cargo_manifest.as_path())
            })
        })
        .ok_or_else(|| {
            format!(
                "cargo metadata did not include the package at '{}'",
                cargo_manifest.display()
            )
        })?;
    let binary = select_binary(package)?;

    let status = Command::new("cargo")
        .args(["run", "--manifest-path"])
        .arg(&cargo_manifest)
        .args(["--bin", &binary, "--"])
        .args(&arguments.arguments)
        .current_dir(project_root)
        .status()
        .map_err(|error| format!("cannot start the project's Cargo host: {error}"))?;
    Ok(ExitCode::from(
        status.code().unwrap_or(1).clamp(0, u8::MAX as i32) as u8,
    ))
}

fn select_binary(package: &Value) -> Result<String, String> {
    let targets = package
        .get("targets")
        .and_then(Value::as_array)
        .ok_or_else(|| "cargo metadata package has no targets array".to_owned())?;
    let binaries: Vec<_> = targets
        .iter()
        .filter(|target| {
            target
                .get("kind")
                .and_then(Value::as_array)
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"))
        })
        .collect();
    if let Some(default_run) = package.get("default_run").and_then(Value::as_str) {
        if binaries
            .iter()
            .any(|target| target.get("name").and_then(Value::as_str) == Some(default_run))
        {
            return Ok(default_run.to_owned());
        }
        return Err(format!(
            "Cargo default-run target `{default_run}` is not a binary target"
        ));
    }
    match binaries.as_slice() {
        [binary] => binary
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| "Cargo binary target has no name".to_owned()),
        [] => Err("project has no Rust CLI target; add a binary target first".to_owned()),
        _ => Err(
            "project has multiple Rust CLI targets; set `package.default-run` in Cargo.toml"
                .to_owned(),
        ),
    }
}

fn print_child_output(output: &Output) {
    use std::io::Write as _;

    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    let _ = stdout.write_all(&output.stdout);
    let _ = stderr.write_all(&output.stderr);
}

#[cfg(test)]
mod tests {
    use super::select_binary;
    use serde_json::json;

    #[test]
    fn binary_selection_honors_default_and_unambiguous_targets() {
        let package = json!({
            "default_run": "runner",
            "targets": [
                {"name": "runner", "kind": ["bin"]},
                {"name": "library", "kind": ["lib"]}
            ]
        });
        assert_eq!(select_binary(&package).unwrap(), "runner");

        let one_binary = json!({
            "default_run": null,
            "targets": [{"name": "runner", "kind": ["bin"]}]
        });
        assert_eq!(select_binary(&one_binary).unwrap(), "runner");
    }

    #[test]
    fn binary_selection_reports_missing_and_ambiguous_cli_targets() {
        let no_binary = json!({"targets": [{"name": "library", "kind": ["lib"]}]});
        assert!(
            select_binary(&no_binary)
                .unwrap_err()
                .contains("no Rust CLI target")
        );

        let ambiguous = json!({
            "targets": [
                {"name": "first", "kind": ["bin"]},
                {"name": "second", "kind": ["bin"]}
            ]
        });
        assert!(
            select_binary(&ambiguous)
                .unwrap_err()
                .contains("set `package.default-run`")
        );
    }
}
