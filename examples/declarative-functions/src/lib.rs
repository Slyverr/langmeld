mod analysis;
mod ast;

pub mod lsp;

pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/langmeld_bindings.rs"));
    include!(concat!(env!("OUT_DIR"), "/langmeld_sources.rs"));
}

use langmeld_core::{Diagnostic, SourceFile};
use langmeld_language::{LanguageSpec, ParsedDocument, parse};
use langmeld_project::load_language_spec_from_sources;

pub use analysis::{FunctionFacts, FunctionSignature};
pub use ast::{CallSite, Function, Parameter, Program};

pub struct FunctionAnalysis {
    pub parsed: ParsedDocument<Program>,
    pub semantic: langmeld_language::AnalysisDocument<FunctionFacts>,
}

/// Loads and validates the function-language definition.
///
/// # Errors
///
/// Returns an error when the project or its definition cannot be loaded, or
/// when the definition does not produce a valid language specification.
pub fn load() -> Result<LanguageSpec, String> {
    load_language_spec_from_sources(generated::LANGMELD_ROOT_KEY, generated::LANGMELD_SOURCES)
        .map_err(|error| error.to_string())
}

/// Parses, lowers, and analyzes one source document.
///
/// Parse and semantic findings are retained in the returned analysis.
///
/// # Errors
///
/// Returns an error when the project's language definition cannot be loaded.
pub fn analyze(source: &SourceFile) -> Result<FunctionAnalysis, String> {
    let language = load()?;
    let generic = parse(source, &language);
    let (program, lowering_diagnostics) = ast::lower(source, &generic);
    let mut parse_diagnostics = generic.diagnostics().to_vec();
    parse_diagnostics.extend(lowering_diagnostics);
    let parsed = ParsedDocument::new(source, program, parse_diagnostics);
    let semantic = analysis::analyze(source, &parsed);

    Ok(FunctionAnalysis { parsed, semantic })
}

pub fn diagnostics(analysis: &FunctionAnalysis) -> Vec<Diagnostic> {
    let mut diagnostics = analysis.parsed.diagnostics().to_vec();
    diagnostics.extend(analysis.semantic.diagnostics().iter().cloned());
    diagnostics
}
