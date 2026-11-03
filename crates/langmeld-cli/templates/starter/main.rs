use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};
use langmeld::core::{Severity, SourceFile};
use langmeld::language::{
    ConfiguredLanguageParser, PreparedLanguage, analyze_workspace, load_language, load_project,
    load_source_files,
};

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
        let language = load_language(Path::new("language/language.lm"))
            .map_err(|error| (2, error))?;
        let sources = load_source_files(&input.sources).map_err(|error| (2, error))?;
        (language, sources)
    };
    if !language.spec().behaviors.is_empty() {
        return Err((
            2,
            "this language declares host analysis behaviors; register them in the Rust host before checking"
                .to_owned(),
        ));
    }
    let parser = PreparedLanguage::new(language.clone())
        .map_err(|error| (2, error.to_string()))?
        .with_custom_parsers([])
        .map_err(|error| (2, error.to_string()))?;
    check_sources(&language, &parser, &sources)?;
    if check_only {
        println!("checked {} source file(s)", sources.len());
        return Ok(());
    }
    @CRATE@::run_sources(&sources).map_err(|error| (2, error))
}

fn check_sources(
    language: &langmeld::core::ValidatedLanguageSpec,
    parser: &ConfiguredLanguageParser,
    sources: &[SourceFile],
) -> Result<(), (u8, String)> {
    let parsed: Vec<_> = sources.iter().map(|source| parser.parse(source)).collect();
    let files: Vec<_> = sources
        .iter()
        .zip(&parsed)
        .map(|(source, result)| (source, &result.program))
        .collect();
    let mut diagnostics: Vec<_> = parsed
        .iter()
        .map(|result| result.diagnostics.clone())
        .collect();
    for item in analyze_workspace(language, &files) {
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
        Err((1, "source contains errors".to_owned()))
    } else {
        Ok(())
    }
}
