mod analysis;
mod ast;

use std::path::Path;

use langmeld_core::{Diagnostic, SourceFile};
use langmeld_language::{LanguageSpec, ParsedDocument, parse};
use langmeld_project::load_definition_project;

pub use analysis::{FunctionFacts, FunctionSignature};
pub use ast::{CallSite, Function, Parameter, Program};

pub struct FunctionAnalysis {
    pub parsed: ParsedDocument<Program>,
    pub semantic: langmeld_language::AnalysisDocument<FunctionFacts>,
}

pub fn load() -> Result<LanguageSpec, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let project = load_definition_project(root).map_err(|error| error.to_string())?;
    let (_, _, definition, sources) = project.into_parts();

    if !definition.is_valid() {
        return Err(langmeld_core::format_diagnostics(
            &sources,
            &definition.diagnostics,
        ));
    }

    definition
        .language
        .ok_or_else(|| "validated definition did not produce a language".to_owned())
}

pub fn analyze(source: &SourceFile) -> Result<FunctionAnalysis, String> {
    let language = load()?;
    let generic = parse(source, &language);
    let (program, lowering_diagnostics) = ast::lower(source, &generic);
    let mut parse_diagnostics = generic.diagnostics;
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
