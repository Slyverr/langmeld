//! The command line: the standard commands and `run`.

use std::path::PathBuf;

use langmeld::ast::AstNode;
use langmeld::cli::clap::{Arg, ArgMatches, Command, value_parser};
use langmeld::cli::{App, Failure, line, parse_file, report};
use langmeld::{FileId, Language};

use crate::interp::{self, EXPLANATIONS};
use crate::syntax::{self, File};

fn run_command(language: &Language, matches: &ArgMatches) -> Result<(), Failure> {
    let path = matches
        .get_one::<PathBuf>("file")
        .ok_or_else(|| Failure::Unable("name a file to run".to_owned()))?;
    let (source, parsed) = parse_file(language, path, FileId::new(0))?;
    let files = std::slice::from_ref(&source);
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

/// A program with the commands added to it so far: none.
fn program(name: &str) -> App<'static> {
    let app = App::new(syntax::language())
        .name(name)
        .version(env!("CARGO_PKG_VERSION"))
        .about("{{Name}}, a language built with Langmeld");
    EXPLANATIONS
        .into_iter()
        .fold(app, |app, (code, text)| app.explanation(code, text))
}

/// Adds `run`.
fn running(app: App<'static>) -> App<'static> {
    let file = Arg::new("file")
        .required(true)
        .value_parser(value_parser!(PathBuf));
    app.command(
        Command::new("run").about("Run a program").arg(file),
        |matches| run_command(syntax::language(), matches),
    )
}

/// The program with every command.
pub fn app() -> App<'static> {
    running(program("{{binary}}").standard())
}

/// A program that only runs programs.
pub fn runner() -> App<'static> {
    running(program("{{binary}}"))
}

/// A program with the commands for working on programs.
pub fn tools() -> App<'static> {
    program("{{binary}}c").standard()
}
