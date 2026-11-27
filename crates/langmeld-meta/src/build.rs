//! Compiling the grammar of a language ahead of time, from a build script, so that the
//! program starts without reading or checking a grammar.
//!
//! ```no_run
//! // build.rs
//! langmeld_meta::build::embed().expect("the grammar compiles");
//! ```
//!
//! [`generate`] does the same and also writes the Rust types of the language's trees,
//! made by [`codegen`](crate::codegen).
//!
//! The program then includes the result:
//!
//! ```ignore
//! let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/language.bin"));
//! let loaded = load_language(Source::embedded(bytes))?;
//! ```

use serde::{Deserialize, Serialize};

use langmeld_syntax::{Diagnostic, EmbedError, Externals, FileId, Language, TextRange, TextSize};

use crate::declared::{BindRules, Editor};
use crate::load::{LoadError, Loaded};
use crate::problem::problem;

/// What a compiled language holds besides the language.
#[derive(Serialize, Deserialize)]
struct Package {
    language: Vec<u8>,
    extensions: Vec<String>,
    bindings: BindRules,
    editor: Editor,
}

fn unreadable(error: &EmbedError) -> LoadError {
    let found: Diagnostic = problem(
        "LM-F015",
        FileId::new(0),
        TextRange::empty(TextSize::from(0)),
        format!("the compiled language cannot be read: {error}"),
        "compile it again with this version of Langmeld, and give the code of every `extern` \
         declaration",
    );
    LoadError {
        files: Vec::new(),
        diagnostics: vec![found],
    }
}

/// Reads a language written by [`write`].
pub(crate) fn read(bytes: &[u8], externals: &Externals) -> Result<Loaded, LoadError> {
    let package: Package =
        postcard::from_bytes(bytes).map_err(|_| unreadable(&EmbedError::Damaged))?;
    let language =
        Language::from_bytes(&package.language, externals).map_err(|error| unreadable(&error))?;
    Ok(Loaded {
        language,
        files: Vec::new(),
        extensions: package.extensions,
        bindings: package.bindings,
        editor: package.editor,
    })
}

/// Writes a loaded language as bytes that [`Source::embedded`](crate::Source::embedded)
/// reads back.
pub fn write(loaded: &Loaded) -> Result<Vec<u8>, EmbedError> {
    let package = Package {
        language: loaded.language.to_bytes()?,
        extensions: loaded.extensions.clone(),
        bindings: loaded.bindings.clone(),
        editor: loaded.editor.clone(),
    };
    postcard::to_stdvec(&package).map_err(|error| EmbedError::Write(error.to_string()))
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{
    BuildError, Compiled, OUTPUT_NAME, SYNTAX_NAME, compile, compile_with_types, embed, generate,
};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::error::Error;
    use std::fmt;
    use std::path::{Path, PathBuf};

    use langmeld_syntax::{FileId, SourceFile};

    use crate::manifest::{FILE_NAME, Manifest};
    use crate::{Loaded, Source, codegen, load_language, place, render};

    /// The name of the file [`embed`] writes into the output folder.
    pub const OUTPUT_NAME: &str = "language.bin";

    /// The name of the file [`generate`] writes the types of the language to.
    pub const SYNTAX_NAME: &str = "syntax.rs";

    /// What compiling a grammar did.
    #[derive(Debug)]
    #[non_exhaustive]
    pub struct Compiled {
        /// Every file read.
        pub read: Vec<String>,
        /// Problems that do not stop the language from being used, one per line.
        pub warnings: Vec<String>,
    }

    /// Why a grammar could not be compiled.
    #[derive(Debug)]
    #[non_exhaustive]
    pub enum BuildError {
        /// A variable that Cargo gives to build scripts is not set.
        NotABuildScript(&'static str),
        /// The grammar or the manifest has these problems, as text.
        Problems(String),
        /// A file could not be read or written.
        Io(String),
    }

    impl fmt::Display for BuildError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::NotABuildScript(variable) => {
                    write!(f, "`{variable}` is not set; call this from a build script")
                }
                Self::Problems(text) => write!(f, "the grammar cannot be compiled\n\n{text}"),
                Self::Io(reason) => f.write_str(reason),
            }
        }
    }

    impl Error for BuildError {}

    /// A path as grammar files write it, with `/`.
    fn shown(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    /// The root file of the project in `project`, and the manifest if there is one.
    fn root_of(project: &Path, read: &mut Vec<String>) -> Result<PathBuf, BuildError> {
        let path = project.join(FILE_NAME);
        if !path.is_file() {
            return Ok(project.join(Manifest::default().root));
        }
        let text = std::fs::read_to_string(&path).map_err(|problem| {
            BuildError::Io(format!("cannot read {}: {problem}", path.display()))
        })?;
        let file = SourceFile::new(FileId::new(0), shown(&path), text)
            .map_err(|problem| BuildError::Io(problem.to_string()))?;
        read.push(shown(&path));
        let manifest = Manifest::parse(&file).map_err(|problems| {
            BuildError::Problems(render(std::slice::from_ref(&file), &problems))
        })?;
        Ok(project.join(manifest.root))
    }

    /// Reads and checks the language project in the folder `project`.
    fn load(project: &Path) -> Result<(Loaded, Compiled), BuildError> {
        let mut read = Vec::new();
        let root = root_of(project, &mut read)?;
        let loaded = load_language(Source::path(&root).generic())
            .map_err(|error| BuildError::Problems(error.rendered()))?;
        read.extend(loaded.files.iter().map(|file| file.name().to_owned()));
        let warnings = loaded
            .language
            .warnings()
            .iter()
            .map(|found| {
                let at = loaded
                    .files
                    .iter()
                    .find(|file| file.id() == found.file)
                    .map(|file| {
                        let start = usize::try_from(u32::from(found.range.start())).unwrap_or(0);
                        format!("{}: ", place(file, start))
                    })
                    .unwrap_or_default();
                format!("{at}{}: {}", found.code, found.message)
            })
            .collect();
        Ok((loaded, Compiled { read, warnings }))
    }

    fn write(path: &Path, bytes: &[u8]) -> Result<(), BuildError> {
        std::fs::write(path, bytes).map_err(|problem| {
            BuildError::Io(format!("cannot write {}: {problem}", path.display()))
        })
    }

    fn write_language(loaded: &Loaded, out: &Path) -> Result<(), BuildError> {
        let bytes = super::write(loaded).map_err(|problem| BuildError::Io(problem.to_string()))?;
        write(out, &bytes)
    }

    /// Checks the grammar of the language project in the folder `project` and writes it,
    /// compiled, to the file `out`. The code of `extern` declarations is not needed here;
    /// the program gives it when it includes the result.
    pub fn compile(project: &Path, out: &Path) -> Result<Compiled, BuildError> {
        let (loaded, compiled) = load(project)?;
        write_language(&loaded, out)?;
        Ok(compiled)
    }

    /// Like [`compile`], and also writes the Rust types of the language, made by
    /// [`codegen`], into the folder `out`: the language as [`OUTPUT_NAME`] and the types
    /// as [`SYNTAX_NAME`].
    pub fn compile_with_types(project: &Path, out: &Path) -> Result<Compiled, BuildError> {
        let (loaded, mut compiled) = load(project)?;
        let code = codegen::generate(&loaded.language)
            .map_err(|problem| BuildError::Problems(problem.to_string()))?;
        compiled.warnings.extend(code.warnings);
        write(&out.join(SYNTAX_NAME), code.source.as_bytes())?;
        write_language(&loaded, &out.join(OUTPUT_NAME))?;
        Ok(compiled)
    }

    /// A folder that Cargo names in a variable.
    fn folder(name: &'static str) -> Result<PathBuf, BuildError> {
        std::env::var_os(name)
            .map(PathBuf::from)
            .ok_or(BuildError::NotABuildScript(name))
    }

    /// Tells Cargo what was read, and shows the warnings.
    fn tell_cargo(compiled: &Compiled) -> Result<(), BuildError> {
        let mut output = std::io::stdout().lock();
        let lines = compiled
            .read
            .iter()
            .map(|path| format!("cargo::rerun-if-changed={path}"))
            .chain(
                compiled
                    .warnings
                    .iter()
                    .map(|warning| format!("cargo::warning={warning}")),
            );
        for line in lines {
            std::io::Write::write_all(&mut output, format!("{line}\n").as_bytes())
                .map_err(|problem| BuildError::Io(problem.to_string()))?;
        }
        Ok(())
    }

    /// For a build script: compiles the grammar of the package into
    /// `$OUT_DIR/language.bin`, shows warnings through Cargo, and has Cargo run the
    /// script again when a file it read changes.
    pub fn embed() -> Result<(), BuildError> {
        let project = folder("CARGO_MANIFEST_DIR")?;
        let compiled = compile(&project, &folder("OUT_DIR")?.join(OUTPUT_NAME))?;
        tell_cargo(&compiled)
    }

    /// For a build script: like [`embed`], and also writes the types of the language into
    /// `$OUT_DIR/syntax.rs`, which the package includes in a module:
    ///
    /// ```ignore
    /// pub mod syntax {
    ///     langmeld::include_syntax!();
    /// }
    /// ```
    pub fn generate() -> Result<(), BuildError> {
        let project = folder("CARGO_MANIFEST_DIR")?;
        let compiled = compile_with_types(&project, &folder("OUT_DIR")?)?;
        tell_cargo(&compiled)
    }
}
