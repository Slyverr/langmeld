//! Ordered, host-owned analysis passes for parser results.

// Returning the shared diagnostic type directly keeps source errors ergonomic.
#![allow(clippy::result_large_err)]

use langmeld_core::{Diagnostic, SourceFile, SourceIdentity, Span};

use crate::{AnalysisDocument, ParseResult, ParsedDocument};

/// Source identity and diagnostics exposed by a parser result to analysis.
pub trait AnalysisInput {
    /// Returns the source snapshot identity associated with this parse result.
    fn source_identity(&self) -> SourceIdentity;

    /// Returns syntax findings retained by this parse result.
    fn diagnostics(&self) -> &[Diagnostic];
}

impl<T> AnalysisInput for ParsedDocument<T> {
    fn source_identity(&self) -> SourceIdentity {
        ParsedDocument::source_identity(self)
    }

    fn diagnostics(&self) -> &[Diagnostic] {
        ParsedDocument::diagnostics(self)
    }
}

impl AnalysisInput for ParseResult {
    fn source_identity(&self) -> SourceIdentity {
        ParseResult::source_identity(self)
    }

    fn diagnostics(&self) -> &[Diagnostic] {
        ParseResult::diagnostics(self)
    }
}

/// One optional semantic phase over a language's parser result.
///
/// The pass owns its policy and contributes findings without prescribing a
/// symbol model, type system, or execution strategy. Passes are run in the
/// order selected by the host.
pub trait LanguagePass<Input, State>
where
    Input: AnalysisInput,
{
    /// Analyzes the current source and parse result, updating host-owned state.
    ///
    /// Return diagnostics for this phase. Later passes still run when a pass
    /// returns diagnostics; hosts can inspect prior state or parse findings and
    /// decide whether a particular phase should continue.
    fn run(&self, source: &SourceFile, parsed: &Input, state: &mut State) -> Vec<Diagnostic>;
}

/// Runs explicitly ordered semantic passes over one parser result.
///
/// The state type and pass sequence are selected by the host language. This
/// context checks source association and collects diagnostics without
/// converting or cloning the parser's result.
pub struct AnalysisContext<'a, Input, State> {
    source: &'a SourceFile,
    parsed: &'a Input,
    state: State,
    diagnostics: Vec<Diagnostic>,
}

impl<'a, Input, State> AnalysisContext<'a, Input, State>
where
    Input: AnalysisInput,
{
    /// Starts analysis for a source snapshot and its parser result.
    ///
    /// The source identity must match for database-managed and standalone
    /// snapshots alike.
    ///
    /// # Errors
    ///
    /// Returns `LM-ANALYSIS-001` when the parse result belongs to a different
    /// source snapshot.
    pub fn new(
        source: &'a SourceFile,
        parsed: &'a Input,
        state: State,
    ) -> Result<Self, Diagnostic> {
        if parsed.source_identity() != source.identity() {
            return Err(Diagnostic::error(
                "LM-ANALYSIS-001",
                "parse result belongs to a different source snapshot",
                source,
                Span::default(),
            ));
        }

        Ok(Self {
            source,
            parsed,
            state,
            diagnostics: Vec::new(),
        })
    }

    /// Runs one pass and appends its diagnostics in pass order.
    pub fn run_pass<P>(&mut self, pass: &P)
    where
        P: LanguagePass<Input, State>,
    {
        self.diagnostics
            .extend(pass.run(self.source, self.parsed, &mut self.state));
    }

    /// Returns the current language-owned analysis state between passes.
    #[must_use]
    pub fn state(&self) -> &State {
        &self.state
    }

    /// Returns diagnostics collected by earlier passes.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Completes analysis with the final host state and collected findings.
    #[must_use]
    pub fn finish(self) -> AnalysisDocument<State> {
        AnalysisDocument::new(self.source, self.state, self.diagnostics)
    }
}
