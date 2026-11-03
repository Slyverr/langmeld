use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};
use langmeld::core::Severity;
use langmeld::language::{load_project, load_source_files};

#[derive(Debug, Parser)]
#[command(about = "Check or run this language project.", subcommand_required = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Parse source files and run generic workspace analysis.
    Check(InputArgs),
    /// Execute source files through the language runtime in src/lib.rs.
    Run(InputArgs),
}

#[derive(Clone, Debug, Default, Args)]
#[command(group(clap::ArgGroup::new("input").multiple(false)))]
struct InputArgs {
    /// Load language and sources from langmeld.json.
    #[arg(long, group = "input", conflicts_with = "sources")]
    project: bool,
    /// Source files to process. Omit with --project.
    #[arg(value_name = "SOURCE", required_unless_present = "project")]
    sources: Vec<PathBuf>,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let (check_only, input) = match cli.command {
        Command::Check(input) => (true, input),
        Command::Run(input) => (false, input),
    };
    match process(input, check_only) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err((code, message)) => {
            eprintln!("{message}");
            std::process::ExitCode::from(code)
        }
    }
}

fn process(input: InputArgs, check_only: bool) -> Result<(), (u8, String)> {
    let (language, sources) = if input.project {
        let project = load_project(Path::new("langmeld.json")).map_err(|error| (2, error))?;
        (project.language, project.sources)
    } else {
        let language = @CRATE@::load_language().map_err(|errors| {
            (
                2,
                errors
                    .into_iter()
                    .map(|error| format!("{}: {}", error.code, error.message))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        })?;
        let sources = load_source_files(&input.sources).map_err(|error| (2, error))?;
        (language, sources)
    };
    let parser = @CRATE@::parser_for(language.clone()).map_err(|errors| {
        (
            2,
            errors
                .into_iter()
                .map(|error| format!("{}: {}", error.code, error.message))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    })?;
    let parsed: Vec<_> = sources.iter().map(|source| parser.parse(source)).collect();
    let programs: Vec<_> = parsed.iter().map(|result| &result.program).collect();
    let analysis = @CRATE@::analyze_sources(&language, &sources, &programs)
        .map_err(|error| (2, error))?;
    let mut diagnostics: Vec<_> = parsed
        .into_iter()
        .map(|result| result.diagnostics)
        .collect();
    for item in analysis.diagnostics {
        diagnostics[item.source_index].push(item.diagnostic);
    }

    let mut has_errors = false;
    for (source, findings) in sources.iter().zip(diagnostics) {
        if findings.is_empty() {
            println!("{}: no issues found", source.name);
            continue;
        }
        for diagnostic in findings {
            has_errors |= diagnostic.severity == Severity::Error;
            let (line, column) = diagnostic
                .span
                .map(|span| source.line_column(span.start))
                .unwrap_or((1, 1));
            println!(
                "{}:{line}:{column}: {:?}[{}]: {}",
                source.name, diagnostic.severity, diagnostic.code, diagnostic.message
            );
        }
    }
    if has_errors {
        return Err((1, "source contains errors".to_owned()));
    }
    if check_only {
        println!("checked {} source file(s)", sources.len());
        return Ok(());
    }
    @CRATE@::run_sources(&sources).map_err(|error| (2, error))
}
