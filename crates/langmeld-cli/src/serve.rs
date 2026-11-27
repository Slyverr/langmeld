//! `langmeld lsp`, which serves `.langmeld` files to editors.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Mutex;

use langmeld_cli::{Failure, error_line};
use langmeld_ide::{LanguageHooks, Served};
use langmeld_meta::{
    DiskFiles, Files, MANIFEST_NAME, Source, declaration, grammar_editor, grammar_language,
    load_language,
};
use langmeld_syntax::{Diagnostic, Parse, Severity, SourceFile, TextRange, TextSize};

use crate::project;

/// The files on disk, except the open ones, whose text the editor holds.
struct Open(BTreeMap<String, String>);

impl Files for Open {
    fn read(&self, path: &str) -> Result<String, String> {
        match self.0.get(path) {
            Some(text) => Ok(text.clone()),
            None => DiskFiles.read(path),
        }
    }
}

/// The root file of the project a grammar file is in, if it is in one.
fn root_of(path: &str) -> Option<String> {
    let folder = Path::new(path).parent()?;
    let home = folder.ancestors().find(|folder| {
        folder.join(MANIFEST_NAME).is_file() || folder.join("language/main.langmeld").is_file()
    })?;
    let found = project::find(home).ok()?;
    let root: PathBuf = found.root;
    Some(root.display().to_string().replace('\\', "/"))
}

/// Whether two names stand for the same file.
fn same(a: &str, b: &str) -> bool {
    a == b
        || matches!((Path::new(a).canonicalize(), Path::new(b).canonicalize()),
            (Ok(a), Ok(b)) if a == b)
}

/// The problems of the grammar as seen from the open file: those of the whole project if
/// the file is in one, and otherwise those of the file alone.
fn problems(path: &str, file: &SourceFile, texts: &BTreeMap<String, String>) -> Vec<Diagnostic> {
    let open = || Open(texts.clone());
    let load = |root: &str| {
        // The root, too, is read as the editor has it.
        let text = open().read(root).ok()?;
        let source = Source::text(root, text).reading(open()).generic();
        let (read, found) = match load_language(source) {
            Ok(loaded) => (loaded.files.clone(), loaded.language.warnings().to_vec()),
            Err(error) => (error.files, error.diagnostics),
        };
        let this = read.iter().find(|read| same(read.name(), path))?.id();
        let mine = found
            .into_iter()
            .filter(|found| found.file == this && found.severity != Severity::Info)
            .map(|mut found| {
                found.file = file.id();
                found.labels.retain(|label| label.file == this);
                for label in &mut found.labels {
                    label.file = file.id();
                }
                found
            });
        Some(mine.collect())
    };
    root_of(path)
        .filter(|root| root != path)
        .and_then(|root| load(&root))
        .or_else(|| load(path))
        .unwrap_or_default()
}

/// The grammar files that are open, by path, with the text the editor holds.
#[derive(Default)]
struct Grammars {
    open: Mutex<BTreeMap<String, String>>,
}

impl LanguageHooks for Grammars {
    fn diagnostics(&self, file: &SourceFile, parse: &Parse) -> Vec<Diagnostic> {
        // The syntax mistakes of the file are already shown, once.
        if !parse.diagnostics.is_empty() {
            return Vec::new();
        }
        // An unsaved document has an address instead of a path.
        let path = file.name().to_owned();
        if !Path::new(&path).is_absolute() {
            return Vec::new();
        }
        let texts = {
            let mut open = self
                .open
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            open.insert(path.clone(), file.text().to_owned());
            open.clone()
        };
        problems(&path, file, &texts)
    }

    fn closed(&self, name: &str) {
        self.open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(name);
    }

    fn provides_definitions(&self) -> bool {
        true
    }

    fn definition(&self, _: &SourceFile, parse: &Parse, offset: TextSize) -> Option<TextRange> {
        declaration(parse, offset)
    }
}

pub(crate) fn run() -> ExitCode {
    let served = langmeld_ide::serve(
        Served::new(grammar_language()).editor(grammar_editor()),
        Grammars::default(),
    );
    let failure = match served {
        Ok(0) => return ExitCode::SUCCESS,
        Ok(_) => Failure::Found,
        Err(problem) => Failure::Unable(problem.to_string()),
    };
    if let Failure::Unable(reason) = &failure {
        error_line(&format!("error: {reason}"));
    }
    ExitCode::from(failure.code())
}
