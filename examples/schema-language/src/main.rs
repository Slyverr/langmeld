use langmeld_core::SourceFile;
use langmeld_language::LanguageAnalyzer;
use langmeld_schema_language_example::{SchemaAnalyzer, parse_schema};

fn main() {
    let source = SourceFile::new(
        "schema.lang",
        "record User { manager: Manager, id: int } record Manager { user: User }",
    );
    let parsed = parse_schema(&source);
    let analysis = SchemaAnalyzer.analyze(&source, &parsed);

    for diagnostic in parsed.diagnostics().iter().chain(analysis.diagnostics()) {
        eprintln!("{}: {}", diagnostic.code, diagnostic.message);
    }

    if parsed.is_valid() && analysis.is_valid() {
        println!(
            "valid schema: {} record(s)",
            analysis.result().symbols.len()
        );
    }
}
