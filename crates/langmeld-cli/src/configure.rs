use std::{io, io::IsTerminal, path::PathBuf};

use console::style;
use langmeld_language::{
    ProjectManifest, load_project_manifest, load_project_with_manifest, write_project_manifest,
};

use crate::{cli::ConfigureArgs, init::prompt_text};

pub(crate) fn configure_project(options: ConfigureArgs) -> Result<(), String> {
    let interactive = options.interactive || (!options.no_interactive && io::stdin().is_terminal());
    let mut manifest = load_project_manifest(&options.project)?;
    apply_cli_options(&options, &mut manifest);

    if interactive {
        println!(
            "\n  {}  {}\n  {}\n",
            style("◈").cyan().bold(),
            style("CONFIGURE PROJECT").bold(),
            style("Update source discovery for this Langmeld project.").dim()
        );
        collect_interactive_settings(&options, &mut manifest)?;
    } else if !has_configuration_options(&options) {
        return Err(
            "configure needs a terminal prompt or at least one source-discovery option".to_owned(),
        );
    }

    let project = load_project_with_manifest(&options.project, &manifest)?;
    write_project_manifest(&options.project, &manifest)?;
    println!(
        "updated {} ({} source files resolve)",
        options.project.display(),
        project.sources.len()
    );
    Ok(())
}

fn has_configuration_options(options: &ConfigureArgs) -> bool {
    !options.source_roots.is_empty()
        || options.clear_source_roots
        || !options.sources.is_empty()
        || options.clear_sources
        || !options.source_extensions.is_empty()
        || options.inherit_source_extensions
}

fn apply_cli_options(options: &ConfigureArgs, manifest: &mut ProjectManifest) {
    if !options.source_roots.is_empty() {
        manifest.source_roots = options.source_roots.clone();
    } else if options.clear_source_roots {
        manifest.source_roots.clear();
    }
    if !options.sources.is_empty() {
        manifest.sources = options.sources.clone();
    } else if options.clear_sources {
        manifest.sources.clear();
    }
    if !options.source_extensions.is_empty() {
        manifest.source_extensions = options.source_extensions.clone();
    } else if options.inherit_source_extensions {
        manifest.source_extensions.clear();
    }
}

fn collect_interactive_settings(
    options: &ConfigureArgs,
    manifest: &mut ProjectManifest,
) -> Result<(), String> {
    if options.source_roots.is_empty() && !options.clear_source_roots {
        let current = display_path_list(&manifest.source_roots);
        let answer = prompt_text("Source roots (comma-separated; `none` clears)", &current)?;
        manifest.source_roots = parse_path_list(&answer);
    }
    if options.sources.is_empty() && !options.clear_sources {
        let current = display_path_list(&manifest.sources);
        let answer = prompt_text(
            "Explicit source files (comma-separated; `none` clears)",
            &current,
        )?;
        manifest.sources = parse_path_list(&answer);
    }
    if options.source_extensions.is_empty() && !options.inherit_source_extensions {
        println!("Leave extensions inherited unless this project needs an override.");
        let current = if manifest.source_extensions.is_empty() {
            "<inherit from language definition>".to_owned()
        } else {
            manifest.source_extensions.join(",")
        };
        let answer = prompt_text(
            "Source extension overrides (comma-separated; `inherit` clears)",
            &current,
        )?;
        manifest.source_extensions = if answer.eq_ignore_ascii_case("inherit")
            || answer == "<inherit from language definition>"
        {
            Vec::new()
        } else {
            parse_text_list(&answer)
        };
    }
    Ok(())
}

fn parse_path_list(value: &str) -> Vec<PathBuf> {
    parse_text_list(value)
        .into_iter()
        .filter(|value| !value.eq_ignore_ascii_case("none"))
        .map(PathBuf::from)
        .collect()
}

fn parse_text_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn display_path_list(paths: &[PathBuf]) -> String {
    if paths.is_empty() {
        "none".to_owned()
    } else {
        paths
            .iter()
            .map(|path| path.to_string_lossy())
            .collect::<Vec<_>>()
            .join(",")
    }
}
