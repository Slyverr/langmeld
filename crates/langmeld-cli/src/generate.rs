use std::{fs, path::Path};

use langmeld_language::{
    GeneratedArtifactWrite, ProjectLanguageSource, check_generated_artifacts,
    generate_host_scaffold, generate_tooling, load_project_language, load_project_manifest,
    parse_language_definition_project, write_generated_artifacts,
};

use crate::cli::GenerateArgs;

pub(crate) mod host;

pub(crate) fn generate(arguments: GenerateArgs) -> Result<(), String> {
    let root = arguments.project_root;
    let manifest_path = root.join("langmeld.json");
    let language = match fs::symlink_metadata(&manifest_path) {
        Ok(_) => {
            let manifest = load_project_manifest(&manifest_path)?;
            match manifest.language_source {
                ProjectLanguageSource::Declarative { .. } => load_project_language(&manifest_path)?,
                ProjectLanguageSource::RustHost {} => {
                    return Err(
                        "`langmeld generate` does not load Rust-host code; from that project run `cargo run -- generate` (or `cargo run -- generate --check`). Library-only projects can call `langmeld::language::generate_tooling`, `generate_host_scaffold`, and `write_generated_artifacts`."
                            .to_owned(),
                    );
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let definition = root.join("language/language.langmeld");
            parse_language_definition_project(&definition)
                .map_err(|diagnostics| format_diagnostics(&definition, &diagnostics))?
        }
        Err(error) => {
            return Err(format!(
                "cannot inspect project manifest '{}': {error}",
                manifest_path.display()
            ));
        }
    };
    let host_artifacts = generate_host_scaffold(&language);
    let install_plan = arguments
        .install_host
        .then(|| host::prepare_host_install(&host_artifacts, &root))
        .transpose()?;
    let mut files = generate_tooling(&language)
        .map_err(|error| format!("cannot generate language tooling: {error}"))?;
    files.extend(host_artifacts);

    let state = root.join(".langmeld/generated");
    if arguments.check {
        check_generated_artifacts(&root, &files)?;
        println!("generated tooling is up to date ({})", state.display());
        return Ok(());
    }

    match write_generated_artifacts(&root, &files)? {
        GeneratedArtifactWrite::UpToDate { .. } if install_plan.is_none() => {
            println!(
                "generated tooling is already up to date ({})",
                state.display()
            );
            return Ok(());
        }
        GeneratedArtifactWrite::UpToDate { .. } => {}
        GeneratedArtifactWrite::Updated { file_count } => {
            println!(
                "generated {file_count} tooling files in {}",
                state.display()
            );
        }
    }
    if let Some(plan) = install_plan {
        host::apply_host_install(plan)?;
    }
    Ok(())
}

fn format_diagnostics(path: &Path, diagnostics: &[langmeld_core::Diagnostic]) -> String {
    let detail = diagnostics
        .iter()
        .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "cannot load language definition '{}':\n{detail}",
        path.display()
    )
}
