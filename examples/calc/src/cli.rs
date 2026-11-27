//! The `calc` program: the standard commands of a language, and `run` and `eval`.

use std::path::PathBuf;

use langmeld::ast::AstNode;
use langmeld::bind::Project;
use langmeld::cli::clap::{Arg, ArgMatches, Command, value_parser};
use langmeld::cli::{App, Failure, line, read, report};
use langmeld::project::DiskFiles;
use langmeld::{FileId, SourceFile, parse};

use crate::checks::divisions_by_zero;
use crate::interp::{self, EXPLANATIONS};
use crate::syntax::{File, language, loaded};

/// Runs a parsed file, showing what it shows, or why it cannot.
fn run_source(source: &SourceFile) -> Result<(), Failure> {
    let parsed = parse(language(), source);
    let files = std::slice::from_ref(source);
    if !parsed.diagnostics.is_empty() {
        report(files, &parsed.diagnostics);
        return Err(Failure::Found);
    }
    let program = File::cast(parsed.syntax())
        .ok_or_else(|| Failure::Unable("the file could not be read".to_owned()))?;
    interp::run(&program, source.id(), &mut line).map_err(|problem| {
        report(files, std::slice::from_ref(&*problem));
        Failure::Found
    })
}

fn run_command(matches: &ArgMatches) -> Result<(), Failure> {
    let path = matches
        .get_one::<PathBuf>("file")
        .ok_or_else(|| Failure::Unable("name a file to run".to_owned()))?;
    let name = path.display().to_string().replace('\\', "/");
    let mut project = Project::new(loaded());
    let id = project
        .set(&name, read(path)?)
        .ok_or_else(|| Failure::Unable(format!("{name}: the file is too large")))?;
    // The files it uses are read too, whatever they use in turn.
    project.open(&name, &DiskFiles);
    let sources: Vec<SourceFile> = project.files().map(|file| file.source().clone()).collect();
    let problems = project.file(id).map(|file| &file.parse().diagnostics);
    if let Some(problems) = problems.filter(|problems| !problems.is_empty()) {
        report(&sources, problems);
        return Err(Failure::Found);
    }
    interp::run_project(&project, id, &mut line).map_err(|problem| {
        report(&sources, std::slice::from_ref(&*problem));
        Failure::Found
    })
}

fn eval_command(matches: &ArgMatches) -> Result<(), Failure> {
    let text = matches
        .get_one::<String>("expression")
        .map_or("", String::as_str);
    // The last expression may leave its `;` out, and what it gives is shown.
    let text = text.trim().trim_end_matches(';');
    let source = SourceFile::new(FileId::new(0), "<expression>", format!("{text};\n"))
        .map_err(|problem| Failure::Unable(problem.to_string()))?;
    run_source(&source)
}

/// The program, with every command.
pub fn app() -> App<'static> {
    let program = App::new(loaded())
        .resolver(crate::Builtins)
        .name("calc")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Calc, a small calculator language built with Langmeld")
        .check_with(divisions_by_zero)
        .standard()
        .command(
            Command::new("run")
                .about("Run a Calc file and show what it shows")
                .arg(
                    Arg::new("file")
                        .required(true)
                        .value_parser(value_parser!(PathBuf)),
                ),
            run_command,
        )
        .command(
            Command::new("eval")
                .about("Work out an expression, such as \"1 + 2 * 3\"")
                .arg(Arg::new("expression").required(true)),
            eval_command,
        );
    EXPLANATIONS
        .into_iter()
        .fold(program, |program, (code, text)| {
            program.explanation(code, text)
        })
}
