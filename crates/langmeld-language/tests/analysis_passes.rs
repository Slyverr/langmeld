use langmeld_core::{Diagnostic, SourceDatabase, SourceFile, Span};
use langmeld_language::{AnalysisContext, LanguagePass, ParsedDocument};

struct RecordPhase(&'static str);

impl LanguagePass<Vec<String>, Vec<&'static str>> for RecordPhase {
    fn run(
        &self,
        _source: &SourceFile,
        _parsed: &ParsedDocument<Vec<String>>,
        state: &mut Vec<&'static str>,
    ) -> Vec<Diagnostic> {
        state.push(self.0);
        Vec::new()
    }
}

struct ReportProblem;

impl LanguagePass<Vec<String>, Vec<&'static str>> for ReportProblem {
    fn run(
        &self,
        source: &SourceFile,
        _parsed: &ParsedDocument<Vec<String>>,
        _state: &mut Vec<&'static str>,
    ) -> Vec<Diagnostic> {
        vec![Diagnostic::error(
            "TEST-001",
            "example finding",
            source,
            Span::new(0, 1),
        )]
    }
}

#[test]
fn runs_passes_in_host_selected_order_and_shares_state() {
    let source = SourceFile::new("main.lang", "x");
    let parsed = ParsedDocument::new(&source, vec!["parsed".to_owned()], Vec::new());
    let mut context = AnalysisContext::new(&source, &parsed, Vec::new())
        .expect("parsed document belongs to the supplied source snapshot");

    context.run_pass(&RecordPhase("declare"));
    context.run_pass(&RecordPhase("resolve"));

    let analysis = context.finish();

    assert_eq!(analysis.result().as_slice(), &["declare", "resolve"]);
    assert!(analysis.diagnostics().is_empty());
    assert_eq!(analysis.source_identity(), source.identity());
}

#[test]
fn later_passes_run_and_diagnostics_keep_their_source_spans() {
    let source = SourceFile::new("main.lang", "x");
    let parsed = ParsedDocument::new(&source, vec!["partial".to_owned()], Vec::new());
    let mut context = AnalysisContext::new(&source, &parsed, Vec::new())
        .expect("parsed document belongs to the supplied source snapshot");

    context.run_pass(&ReportProblem);
    assert_eq!(context.diagnostics().len(), 1);

    context.run_pass(&RecordPhase("continue"));

    let analysis = context.finish();

    assert_eq!(analysis.result().as_slice(), &["continue"]);
    assert_eq!(analysis.diagnostics().len(), 1);
    assert_eq!(analysis.diagnostics()[0].code, "TEST-001");
    assert_eq!(
        analysis.diagnostics()[0]
            .location
            .as_ref()
            .expect("finding should retain its source span")
            .span(),
        Span::new(0, 1)
    );
}

#[test]
fn rejects_parsed_documents_from_another_managed_snapshot() -> Result<(), Box<dyn std::error::Error>>
{
    let mut sources = SourceDatabase::default();
    let source_id = sources.insert("main.lang", "x")?;
    let original_source = sources
        .get(source_id)
        .expect("source was just inserted")
        .clone();
    let parsed = ParsedDocument::new(&original_source, vec!["old".to_owned()], Vec::new());

    assert!(AnalysisContext::new(&original_source, &parsed, Vec::<String>::new()).is_ok());

    sources.update(source_id, "y")?;
    let current_source = sources.get(source_id).expect("source still exists");
    let error = AnalysisContext::new(current_source, &parsed, Vec::<String>::new())
        .err()
        .expect("stale parsed documents must be rejected");

    assert_eq!(error.code, "LM-ANALYSIS-001");

    Ok(())
}

#[test]
fn rejects_parsed_documents_from_another_standalone_snapshot() {
    let parsed_source = SourceFile::new("main.lang", "x");
    let current_source = SourceFile::new("main.lang", "x");
    let parsed = ParsedDocument::new(&parsed_source, vec!["old".to_owned()], Vec::new());

    let error = AnalysisContext::new(&current_source, &parsed, Vec::<String>::new())
        .err()
        .expect("standalone source identities must be checked");

    assert_eq!(error.code, "LM-ANALYSIS-001");
}
