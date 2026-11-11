//! Contracts for host-owned parsers and parse trees.

use langmeld_core::{Diagnostic, SourceFile, SourceIdentity, SourceLocation, has_errors};

use crate::CompletionCandidate;

/// A host parser's tree and findings for one immutable source snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedDocument<T> {
    /// Identity and revision of the source snapshot this parse belongs to.
    source: SourceIdentity,
    /// Parser-owned syntax tree or intermediate representation.
    tree: T,
    /// Lexical and syntactic findings associated with the parse.
    diagnostics: Vec<Diagnostic>,
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

    /// Returns the identity and revision of the source snapshot this parse belongs to.
    #[must_use]
    pub const fn source_identity(&self) -> SourceIdentity {
        self.source
    }

    /// Returns the parser-owned syntax tree or intermediate representation.
    #[must_use]
    pub const fn tree(&self) -> &T {
        &self.tree
    }

    /// Returns the lexical and syntactic findings associated with this parse.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Consumes the parse and returns its source identity, tree, and findings.
    #[must_use]
    pub fn into_parts(self) -> (SourceIdentity, T, Vec<Diagnostic>) {
        (self.source, self.tree, self.diagnostics)
    }

    /// Returns whether parsing produced no error diagnostics.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !has_errors(&self.diagnostics)
    }
}

/// A host-owned semantic result and findings for one source snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisDocument<T> {
    /// Identity and revision of the source snapshot this analysis belongs to.
    source: SourceIdentity,
    /// Language-specific semantic result, such as symbols or typed facts.
    result: T,
    /// Semantic findings associated with the source snapshot.
    diagnostics: Vec<Diagnostic>,
}

impl<T> AnalysisDocument<T> {
    /// Creates an analysis document tied to the supplied source snapshot.
    #[must_use]
    pub fn new(source: &SourceFile, result: T, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            source: source.identity(),
            result,
            diagnostics,
        }
    }

    /// Returns the identity and revision of the source snapshot this analysis belongs to.
    #[must_use]
    pub const fn source_identity(&self) -> SourceIdentity {
        self.source
    }

    /// Returns the language-specific semantic result.
    #[must_use]
    pub const fn result(&self) -> &T {
        &self.result
    }

    /// Returns the semantic findings associated with this source snapshot.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Consumes the analysis and returns its source identity, result, and findings.
    #[must_use]
    pub fn into_parts(self) -> (SourceIdentity, T, Vec<Diagnostic>) {
        (self.source, self.result, self.diagnostics)
    }

    /// Returns whether analysis produced no error diagnostics.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !has_errors(&self.diagnostics)
    }
}

/// Parsing behavior that retains a host language's own syntax tree.
///
/// Implementations own grammar, recovery, and tree shape. Langmeld does not
/// infer editor operations from an arbitrary tree; hosts can implement the
/// completion and navigation contracts for their own syntax and semantics.
/// The returned document must retain the identity of the supplied source.
pub trait LanguageParser {
    /// Host-owned syntax tree returned by this parser.
    type Tree;

    /// Parses one immutable source snapshot.
    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree>;
}

/// Optional semantic analysis for a language parser's host-owned syntax tree.
///
/// The output type and semantic policies belong to the host language. Simple
/// languages can omit this stage, while analyzers can inspect partial parse
/// trees and parse diagnostics when recovery produced useful syntax.
/// Check that the parsed document belongs to `source` before using its tree;
/// [`crate::AnalysisContext::new`] performs this check for pass-based analyzers.
/// The returned document must retain the identity of the supplied source.
pub trait LanguageAnalyzer<P: LanguageParser> {
    /// Language-specific analysis result.
    type Analysis;

    /// Analyzes one parsed source snapshot.
    fn analyze(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<P::Tree>,
    ) -> AnalysisDocument<Self::Analysis>;
}

/// Optional semantic analysis across several parsed source snapshots.
///
/// Implement this when references or validation span multiple documents.
/// Each parsed document must belong to its paired source.
/// Each returned result must correspond to the source at the same input index
/// and retain that source's identity. Hosts that need to load unopened files or
/// retain project indexes can implement the lower-level workspace backend.
/// The existing per-source analyzer is passed in so workspace analysis can
/// reuse its checks and combine them with cross-document rules.
pub trait LanguageWorkspaceAnalyzer<P: LanguageParser, A: LanguageAnalyzer<P>> {
    /// Analyzes open source snapshots and their already parsed trees together.
    fn analyze_workspace(
        &self,
        analyzer: &A,
        documents: &[(&SourceFile, &ParsedDocument<P::Tree>)],
    ) -> Vec<AnalysisDocument<A::Analysis>>;
}

/// Host-defined completion behavior for a parser-owned syntax tree.
///
/// The provider receives the existing parse and optional semantic result so
/// editor integrations can avoid parsing or analyzing the same snapshot again.
/// Use `()` as `Analysis` when completion does not depend on semantic analysis.
pub trait LanguageCompleter<P: LanguageParser> {
    /// Language-specific semantic result used by this provider.
    type Analysis;

    /// Returns candidates valid at a UTF-8 byte offset in `source`.
    fn complete(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<P::Tree>,
        analysis: Option<&AnalysisDocument<Self::Analysis>>,
        offset: usize,
    ) -> Vec<CompletionCandidate>;
}

/// Host-defined symbol navigation for a parser-owned syntax tree.
///
/// Name resolution stays with the host language, which can use lexical scopes,
/// global namespaces, or another policy. Results use source locations so
/// editor adapters can navigate without interpreting the host's analysis type.
pub trait LanguageNavigator<P: LanguageParser> {
    /// Language-specific semantic result used by this provider.
    type Analysis;

    /// Finds declaration locations for the symbol at `offset`.
    fn definition(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<P::Tree>,
        analysis: Option<&AnalysisDocument<Self::Analysis>>,
        offset: usize,
    ) -> Vec<SourceLocation>;

    /// Finds uses of the symbol at `offset`, optionally including its declaration.
    fn references(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<P::Tree>,
        analysis: Option<&AnalysisDocument<Self::Analysis>>,
        offset: usize,
        include_declaration: bool,
    ) -> Vec<SourceLocation>;
}
