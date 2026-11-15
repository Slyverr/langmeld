use langmeld_core::SourceFile;
use langmeld_language::{
    AnalysisDocument, CompletionCandidate, LanguageAnalyzer, LanguageCompleter, LanguageParser,
    LanguageSpec, ParsedDocument, complete_at, parse,
};
use langmeld_lsp::{HostLanguageBackend, LanguageBackend};

use crate::{FunctionFacts, Program, analysis, ast};

struct FunctionFlowParser {
    language: LanguageSpec,
}

impl LanguageParser for FunctionFlowParser {
    type Tree = Program;

    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree> {
        let generic = parse(source, &self.language);
        let (program, lowering_diagnostics) = ast::lower(source, &generic);
        let mut diagnostics = generic.diagnostics().to_vec();
        diagnostics.extend(lowering_diagnostics);

        ParsedDocument::new(source, program, diagnostics)
    }
}

struct FunctionFlowAnalyzer;

impl LanguageAnalyzer<FunctionFlowParser> for FunctionFlowAnalyzer {
    type Analysis = FunctionFacts;

    fn analyze(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<Program>,
    ) -> AnalysisDocument<Self::Analysis> {
        analysis::analyze(source, parsed)
    }
}

struct FunctionFlowCompleter {
    language: LanguageSpec,
}

impl LanguageCompleter<FunctionFlowParser> for FunctionFlowCompleter {
    type Analysis = FunctionFacts;

    fn complete(
        &self,
        source: &SourceFile,
        _parsed: &ParsedDocument<Program>,
        _analysis: Option<&AnalysisDocument<Self::Analysis>>,
        offset: usize,
    ) -> Vec<CompletionCandidate> {
        complete_at(source, &self.language, offset)
    }
}

/// Creates the FunctionFlow language server backend.
///
/// # Errors
///
/// Returns an error if the declarative language definition is invalid.
pub fn backend() -> Result<impl LanguageBackend, String> {
    let language = crate::load()?;
    let parser = FunctionFlowParser {
        language: language.clone(),
    };
    let completer = FunctionFlowCompleter { language };

    Ok(
        HostLanguageBackend::new_with_analyzer(parser, FunctionFlowAnalyzer)
            .with_completer(completer),
    )
}

/// Starts the FunctionFlow language server over standard input and output.
///
/// # Errors
///
/// Returns an error if the language definition cannot be loaded or the server
/// cannot be started.
pub fn run_stdio() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let backend = backend().map_err(std::io::Error::other)?;

    langmeld_lsp::run_stdio(backend)
}
