//! Contracts for host-owned parsers and parse trees.

use langmeld_core::{Diagnostic, SourceFile, SourceIdentity};

/// A host parser's tree and findings for one immutable source snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedDocument<T> {
    /// Identity and revision of the analyzed source, if database-managed.
    pub source: Option<SourceIdentity>,
    /// Parser-owned syntax tree or intermediate representation.
    pub tree: T,
    /// Lexical and syntactic findings associated with the parse.
    pub diagnostics: Vec<Diagnostic>,
}

impl<T> ParsedDocument<T> {
    /// Creates a parse document tied to the supplied source snapshot.
    #[must_use]
    pub fn new(source: &SourceFile, tree: T, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            source: source.identity(),
            tree,
            diagnostics,
        }
    }

    /// Returns whether parsing produced no error diagnostics.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != langmeld_core::Severity::Error)
    }
}

/// Parsing behavior that retains a host language's own syntax tree.
///
/// Implementations own grammar, recovery, and tree shape. Langmeld does not
/// infer editor operations from an arbitrary tree; hosts can adapt their tree
/// to later tooling interfaces when those interfaces are available.
pub trait LanguageParser {
    /// Host-owned syntax tree returned by this parser.
    type Tree;

    /// Parses one immutable source snapshot.
    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree>;
}
