use std::{io, path::PathBuf, process::ExitCode};

use clap::Parser;
use langmeld::language::{WorkspaceTypeAnalysisRegistration, load_project};
use langmeld_lsp::{LanguageServerConfig, serve_configured};
use python_source_language::{configured_parser, project_import_analysis_hook};

fn main() -> ExitCode {
    let arguments = Args::parse();
    let parser = match configured_parser() {
        Ok(parser) => parser,
        Err(diagnostics) => {
            for diagnostic in diagnostics {
                eprintln!("{}: {}", diagnostic.code, diagnostic.message);
            }
            return ExitCode::FAILURE;
        }
    };
    let mut config = LanguageServerConfig::new(parser).with_type_analysis_hooks([
        WorkspaceTypeAnalysisRegistration::new(
            "python_project_imports",
            project_import_analysis_hook(),
        ),
    ]);
    if let Some(root) = &arguments.project_root {
        let manifest_path = root.join("langmeld.json");
        let snapshot = match load_project(&manifest_path) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                eprintln!(
                    "cannot load Python project '{}': {error}",
                    manifest_path.display()
                );
                return ExitCode::FAILURE;
            }
        };
        config = config.with_project_snapshot(manifest_path, snapshot);
    }
    match serve_configured(io::stdin().lock(), io::stdout().lock(), config) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Python source index LSP failed: {error}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Debug, Parser)]
#[command(about = "Serve Python source indexing over the Language Server Protocol.")]
struct Args {
    /// Load project sources from this directory.
    #[arg(long)]
    project_root: Option<PathBuf>,
}
