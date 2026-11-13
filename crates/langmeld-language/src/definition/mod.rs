//! Frontend for the first user-facing `.langmeld` language-definition syntax.

mod grammar;
mod lower;
mod project;
mod strings;
mod unit;

use std::fmt;

use langmeld_core::{Diagnostic, SourceFile, has_errors};

use crate::LanguageSpec;

pub use strings::{decode_definition_string, quote_definition_string};

/// Result of parsing and validating one language-definition source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionResult {
    /// Validated language when the complete definition has no errors.
    pub language: Option<LanguageSpec>,
    /// Parser and definition diagnostics tied to the source snapshot.
    pub diagnostics: Vec<Diagnostic>,
}

impl DefinitionResult {
    /// Returns whether a valid language was produced.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.language.is_some() && !has_errors(&self.diagnostics)
    }
}

/// A named `.langmeld` source supplied to a language-definition project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionSource {
    /// Stable resolver key used to detect duplicate imports and cycles.
    key: String,
    /// Immutable source snapshot containing the definition text.
    source: SourceFile,
}

/// Owned components of a consumed [`DefinitionSource`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionSourceParts {
    /// Stable resolver key used to identify this source.
    pub key: String,
    /// Immutable source snapshot containing the definition text.
    pub source: SourceFile,
}

/// Maximum number of nested definition imports accepted in one project.
pub const MAX_DEFINITION_IMPORT_DEPTH: usize = 64;

impl DefinitionSource {
    /// Creates a source with a stable resolver key.
    #[must_use]
    pub fn new(key: impl Into<String>, source: SourceFile) -> Self {
        Self {
            key: key.into(),
            source,
        }
    }

    /// Returns the stable resolver key for this source.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the immutable source snapshot containing the definition.
    #[must_use]
    pub const fn source(&self) -> &SourceFile {
        &self.source
    }

    /// Consumes this value and returns its named resolver key and source snapshot.
    #[must_use]
    pub fn into_parts(self) -> DefinitionSourceParts {
        DefinitionSourceParts {
            key: self.key,
            source: self.source,
        }
    }
}

/// Host-controlled loading of explicitly imported `.langmeld` files.
pub trait DefinitionResolver {
    /// Error returned when the host cannot resolve an import.
    type Error: fmt::Display;

    /// Resolves `import` relative to `from_key` and returns a stable source key.
    ///
    /// Implementations own filesystem access, path normalization, and any
    /// project-root restrictions.
    ///
    /// # Errors
    ///
    /// Returns the resolver's error when the import cannot be resolved or its
    /// source cannot be loaded.
    fn resolve(&self, from_key: &str, import: &str) -> Result<DefinitionSource, Self::Error>;
}

/// Parses a single language-definition file and validates its declared grammar.
///
/// The bootstrap grammar is authored in Rust and parsed by Langmeld's generic
/// parser. This keeps the definition frontend on the same parser path that
/// user-defined languages use. The accepted subset currently describes
/// keyword-led token patterns and delimited blocks. Files containing imports
/// must use [`parse_language_definition_project`] with a host resolver. Root
/// declarations restrict top-level constructs; without them, every construct
/// remains available at the source root.
#[must_use]
pub fn parse_language_definition(source: &SourceFile) -> DefinitionResult {
    let mut unit = unit::parse_definition_unit(source, true);
    if let Some(import) = unit.imports.first() {
        unit.diagnostics.push(Diagnostic::error(
            "LM-DEF-010",
            "this definition contains imports; use `parse_language_definition_project` with a resolver",
            source,
            import.span,
        ));
    }
    let constructs = unit
        .constructs
        .into_iter()
        .map(|(construct, span)| (construct, source.clone(), span))
        .collect();
    project::finish_definition(
        unit.language_name,
        constructs,
        unit.root_constructs,
        unit.lexical_rules,
        unit.lexical_span,
        unit.diagnostics,
        source.clone(),
    )
}

/// Returns Langmeld's built-in grammar for `.langmeld` language definitions.
///
/// The grammar accepts root declarations and constrains nested syntax through
/// explicit blocks. It is used by the definition parser and can also drive
/// editor features such as diagnostics and context-aware completion.
#[must_use]
pub fn language_definition_grammar() -> &'static LanguageSpec {
    grammar::definition_grammar()
}

pub use project::parse_language_definition_project;
