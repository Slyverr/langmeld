//! A language's command line.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Arg, ArgAction, ArgMatches, Command, value_parser};
use langmeld_ide::Served;
use langmeld_ide::bind::Resolver;
#[cfg(feature = "lsp")]
use langmeld_ide::{ProjectFileKind, Server, StdioTransport};
use langmeld_meta::Editor;
use langmeld_syntax::{Diagnostic, Parse, SourceFile, explain};

use crate::{Failure, Format, check_files_with, line, run_corpus};

type Run<'l> = Box<dyn Fn(&ArgMatches) -> Result<(), Failure> + 'l>;
type Host<'l> = Box<dyn Fn(&SourceFile, &Parse) -> Vec<Diagnostic> + 'l>;

/// The language's checks, shown by editors on files that parse without problems, and
/// its way of resolving names.
#[cfg(feature = "lsp")]
struct Checks<'a, 'l> {
    host: &'a Host<'l>,
    resolver: Option<&'a (dyn Resolver + 'l)>,
}

#[cfg(feature = "lsp")]
impl langmeld_ide::LanguageHooks for Checks<'_, '_> {
    fn diagnostics(&self, file: &SourceFile, parse: &Parse) -> Vec<Diagnostic> {
        if parse.diagnostics.is_empty() {
            (self.host)(file, parse)
        } else {
            Vec::new()
        }
    }

    fn resolver(&self) -> Option<&dyn Resolver> {
        self.resolver.map(|resolver| resolver as &dyn Resolver)
    }
}

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
/// | `lsp` | serves the language to editors on standard input and output (with the `lsp` feature) |
///
/// Every command ends with exit code 0 when all is well, 1 when it found problems and
/// showed them, and 2 when it could not do its work.
pub struct App<'l> {
    served: Served<'l>,
    name: String,
    version: Option<String>,
    about: Option<String>,
    standard: bool,
    commands: Vec<(Command, Run<'l>)>,
    host: Host<'l>,
    resolver: Option<Box<dyn Resolver + 'l>>,
    #[cfg(feature = "lsp")]
    project_files: Vec<ProjectFileKind>,
    explanations: BTreeMap<String, String>,
}

impl<'l> App<'l> {
    /// A program for `language`, named after it, with no commands yet. A language read
    /// from a file is given as it is, such as `syntax::loaded()` of generated code, and
    /// brings the rules that editors use.
    pub fn new(language: impl Into<Served<'l>>) -> Self {
        let served = language.into();
        Self {
            served,
            name: served.language().name().to_lowercase(),
            version: None,
            about: None,
            standard: false,
            commands: Vec::new(),
            host: Box::new(|_, _| Vec::new()),
            resolver: None,
            #[cfg(feature = "lsp")]
            project_files: Vec::new(),
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

    /// What a language that was not read from a file tells editors, which `lsp` serves.
    #[must_use]
    pub fn editor(mut self, editor: &'l Editor) -> Self {
        self.served = self.served.editor(editor);
        self
    }

    /// Adds `check`, `test` and `explain`, and with the `lsp` feature, `lsp`.
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

    /// How `check` and the language server resolve names, where the declared rules are
    /// not enough: names that are there without being written, names a node stands for,
    /// and what a member means.
    #[must_use]
    pub fn resolver(mut self, resolver: impl Resolver + 'l) -> Self {
        self.resolver = Some(Box::new(resolver));
        self
    }

    /// Has the language server check the project file of the language's users called
    /// `name`, such as `mylang.toml`, as the type `T` reads it, and complete its keys.
    #[cfg(feature = "lsp")]
    #[must_use]
    pub fn project_file<T: serde::de::DeserializeOwned>(mut self, name: impl Into<String>) -> Self {
        self.project_files.push(ProjectFileKind::new::<T>(name));
        self
    }

    /// What `explain` says about a diagnostic code of the language's own.
    #[must_use]
    pub fn explanation(mut self, code: impl Into<String>, text: impl Into<String>) -> Self {
        self.explanations.insert(code.into(), text.into());
        self
    }

    fn standard_commands() -> Vec<Command> {
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
        let mut commands = vec![check, test, explain];
        if cfg!(feature = "lsp") {
            // Editors often add `--stdio`, which is the transport there is.
            let stdio = Arg::new("stdio")
                .long("stdio")
                .action(ArgAction::SetTrue)
                .hide(true);
            commands.push(
                Command::new("lsp")
                    .about("Serve the language to editors over standard input and output")
                    .arg(stdio),
            );
        }
        commands
    }

    /// Serves the language to an editor until it asks to exit.
    #[cfg(feature = "lsp")]
    fn lsp(&self) -> Result<(), Failure> {
        let hooks = Checks {
            host: &self.host,
            resolver: self.resolver.as_deref(),
        };
        let server = self
            .project_files
            .iter()
            .cloned()
            .fold(Server::new(self.served, hooks), Server::project_file_of);
        match langmeld_ide::serve_with(StdioTransport::new(), server) {
            Ok(0) => Ok(()),
            Ok(_) => Err(Failure::Found),
            Err(problem) => Err(Failure::Unable(problem.to_string())),
        }
    }

    #[cfg(not(feature = "lsp"))]
    fn lsp(&self) -> Result<(), Failure> {
        Err(Failure::Unable(
            "this program has no language server".to_owned(),
        ))
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
        let resolver = self.resolver.as_deref();
        check_files_with(self.served, &paths, format, &self.host, resolver)
    }

    fn test(&self, matches: &ArgMatches) -> Result<(), Failure> {
        let folder = matches
            .get_one::<PathBuf>("corpus")
            .map_or(Path::new("corpus"), PathBuf::as_path);
        run_corpus(self.served.language(), folder, matches.get_flag("update"))
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
            (None, "lsp") => self.lsp(),
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
