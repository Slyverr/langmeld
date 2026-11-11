//! Ordered, host-owned analysis passes for parsed language documents.

use langmeld_core::{Diagnostic, SourceFile};

use crate::{AnalysisDocument, ParsedDocument};

/// One optional semantic phase over a language's parsed syntax tree.
///
/// The pass owns its policy and contributes findings without prescribing a
/// symbol model, type system, or execution strategy. Passes are run in the
/// order selected by the host.
pub trait LanguagePass<Tree, State> {
    /// Analyzes the current source and parsed tree, updating host-owned state.
    ///
    /// Return diagnostics for this phase. Later passes still run when a pass
    /// returns diagnostics; hosts can inspect prior state or parse findings and
    /// decide whether a particular phase should continue.
    fn run(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<Tree>,
        state: &mut State,
    ) -> Vec<Diagnostic>;
}

/// Runs explicitly ordered semantic passes over one parsed source snapshot.
///
/// The state type and pass sequence are selected by the host language. This
/// context only preserves source association and collects diagnostics.
pub struct AnalysisContext<'a, Tree, State> {
    source: &'a SourceFile,
    parsed: &'a ParsedDocument<Tree>,
    state: State,
    diagnostics: Vec<Diagnostic>,
}

impl<'a, Tree, State> AnalysisContext<'a, Tree, State> {
    /// Starts analysis for an existing source snapshot and parsed document.
    #[must_use]
    pub fn new(source: &'a SourceFile, parsed: &'a ParsedDocument<Tree>, state: State) -> Self {
        Self {
            source,
            parsed,
            state,
            diagnostics: Vec::new(),
        }
    }

    /// Runs one pass and appends its diagnostics in pass order.
    pub fn run_pass<P>(&mut self, pass: &P)
    where
        P: LanguagePass<Tree, State>,
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
