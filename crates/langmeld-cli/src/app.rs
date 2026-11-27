//! A language's command line.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Arg, ArgAction, ArgMatches, Command, value_parser};
use langmeld_syntax::{Diagnostic, Language, Parse, SourceFile, explain};

use crate::{Failure, Format, check_files, line, run_corpus};

type Run<'l> = Box<dyn Fn(&ArgMatches) -> Result<(), Failure> + 'l>;
type Host<'l> = Box<dyn Fn(&SourceFile, &Parse) -> Vec<Diagnostic> + 'l>;

/// The command line of a language. Add the standard commands and your own, then
/// [`run`](Self::run) it.
///
/// The standard commands are:
///
/// | Command | What it does |
/// |---------|--------------|
/// | `check [PATHS]` | parses files, runs the language's own checks and shows every problem; `--format json` writes one JSON object instead |
/// | `test` | runs the corpus files of the language, `--corpus` says where, `--update` rewrites what failing cases expect |
/// | `explain CODE` | says what a diagnostic code means and how to solve it |
///
/// Every command ends with exit code 0 when all is well, 1 when it found problems and
/// showed them, and 2 when it could not do its work.
pub struct App<'l> {
    language: &'l Language,
    name: String,
    version: Option<String>,
    about: Option<String>,
    standard: bool,
    commands: Vec<(Command, Run<'l>)>,
    host: Host<'l>,
    explanations: BTreeMap<String, String>,
}

impl<'l> App<'l> {
    /// A program for `language`, named after it, with no commands yet.
    pub fn new(language: &'l Language) -> Self {
        Self {
            language,
            name: language.name().to_lowercase(),
            version: None,
            about: None,
            standard: false,
            commands: Vec::new(),
            host: Box::new(|_, _| Vec::new()),
            explanations: BTreeMap::new(),
        }
    }

    /// The name of the program.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// The version `--version` prints.
    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// What the program is, shown by `--help`.
    #[must_use]
    pub fn about(mut self, about: impl Into<String>) -> Self {
        self.about = Some(about.into());
        self
    }

    /// Adds `check`, `test` and `explain`.
    #[must_use]
    pub fn standard(mut self) -> Self {
        self.standard = true;
        self
    }

    /// Adds a command of the language's own, described with `clap`. One with the name
    /// of a standard command replaces it.
    #[must_use]
    pub fn command(
        mut self,
        command: Command,
        run: impl Fn(&ArgMatches) -> Result<(), Failure> + 'l,
    ) -> Self {
        self.commands.push((command, Box::new(run)));
        self
    }

    /// The language's own checks, which `check` runs on files that parse without
    /// problems.
    #[must_use]
    pub fn check_with(
        mut self,
        check: impl Fn(&SourceFile, &Parse) -> Vec<Diagnostic> + 'l,
    ) -> Self {
        self.host = Box::new(check);
        self
    }

    /// What `explain` says about a diagnostic code of the language's own.
    #[must_use]
    pub fn explanation(mut self, code: impl Into<String>, text: impl Into<String>) -> Self {
        self.explanations.insert(code.into(), text.into());
        self
    }

    fn standard_commands() -> [Command; 3] {
        let check = Command::new("check")
            .about("Check files and show their problems")
            .arg(
                Arg::new("paths")
                    .value_parser(value_parser!(PathBuf))
                    .num_args(0..)
                    .default_value(".")
                    .help("Files, or folders to search"),
            )
            .arg(
                Arg::new("format")
                    .long("format")
                    .value_parser(["text", "json"])
                    .default_value("text")
                    .help("How to write the problems"),
            );
        let test = Command::new("test")
            .about("Run the corpus files of the language")
            .arg(
                Arg::new("corpus")
                    .long("corpus")
                    .value_parser(value_parser!(PathBuf))
                    .default_value("corpus")
                    .help("The folder of corpus files"),
            )
            .arg(
                Arg::new("update")
                    .long("update")
                    .action(ArgAction::SetTrue)
                    .help("Write what the language gives for failing cases into the files"),
            );
        let explain = Command::new("explain")
            .about("Explain a diagnostic code")
            .arg(Arg::new("code").required(true).help("Such as LM-P001"));
        [check, test, explain]
    }

    fn check(&self, matches: &ArgMatches) -> Result<(), Failure> {
        let paths: Vec<PathBuf> = matches
            .get_many("paths")
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        let format = match matches.get_one::<String>("format").map(String::as_str) {
            Some("json") => Format::Json,
            _ => Format::Text,
        };
        check_files(self.language, &paths, format, &self.host)
    }

    fn test(&self, matches: &ArgMatches) -> Result<(), Failure> {
        let folder = matches
            .get_one::<PathBuf>("corpus")
            .map_or(Path::new("corpus"), PathBuf::as_path);
        run_corpus(self.language, folder, matches.get_flag("update"))
    }

    fn explain(&self, matches: &ArgMatches) -> Result<(), Failure> {
        let code = matches.get_one::<String>("code").map_or("", String::as_str);
        let text = self
            .explanations
            .get(code)
            .map(String::as_str)
            .or_else(|| explain(code))
            .ok_or_else(|| {
                Failure::Unable(format!(
                    "there is no diagnostic code `{code}`; codes look like `LM-P001`"
                ))
            })?;
        line(&format!("{code}\n\n{text}"));
        Ok(())
    }

    /// The program as `clap` describes it.
    fn describe(&self) -> Command {
        let mut program = Command::new(self.name.clone()).subcommand_required(true);
        if let Some(version) = &self.version {
            program = program.version(version.clone());
        }
        if let Some(about) = &self.about {
            program = program.about(about.clone());
        }
        let standard = self
            .standard
            .then(Self::standard_commands)
            .into_iter()
            .flatten()
            .filter(|standard| {
                !self
                    .commands
                    .iter()
                    .any(|(own, _)| own.get_name() == standard.get_name())
            });
        program
            .subcommands(standard)
            .subcommands(self.commands.iter().map(|(command, _)| command.clone()))
    }

    /// Runs the command that `arguments` name, the first being the name the program
    /// was started with, and gives its exit code.
    ///
    /// A program started under the name `name-command`, as by a link of that name,
    /// runs that command, so that one build can serve as several programs.
    pub fn run_from(&self, arguments: impl IntoIterator<Item = OsString>) -> u8 {
        let mut arguments: Vec<_> = arguments.into_iter().collect();
        let program = self.describe();
        let invoked = arguments
            .first()
            .and_then(|path| Path::new(path).file_stem())
            .and_then(OsStr::to_str)
            .and_then(|stem| stem.strip_prefix(&self.name)?.strip_prefix('-'))
            .filter(|command| program.find_subcommand(command).is_some())
            .map(OsString::from);
        if let Some(command) = invoked {
            arguments.insert(1, command);
        }
        let matches = match program.try_get_matches_from(arguments) {
            Ok(matches) => matches,
            Err(problem) => {
                // Help and versions come this way too, and succeed.
                let _ = problem.print();
                return u8::try_from(problem.exit_code()).unwrap_or(2);
            }
        };
        let Some((name, matches)) = matches.subcommand() else {
            return 2;
        };
        let own = self
            .commands
            .iter()
            .find(|(command, _)| command.get_name() == name);
        let result = match (own, name) {
            (Some((_, run)), _) => run(matches),
            (None, "check") => self.check(matches),
            (None, "test") => self.test(matches),
            (None, "explain") => self.explain(matches),
            (None, other) => Err(Failure::Unable(format!("no command `{other}`"))),
        };
        match result {
            Ok(()) => 0,
            Err(failure) => {
                if let Failure::Unable(reason) = &failure {
                    crate::error_line(&format!("error: {reason}"));
                }
                failure.code()
            }
        }
    }

    /// Runs the command that the program was started with.
    pub fn run(&self) -> ExitCode {
        ExitCode::from(self.run_from(std::env::args_os()))
    }
}
