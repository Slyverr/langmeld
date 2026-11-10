use langmeld_core::SourceFile;
use langmeld_schema_language_example::{analyze, parse_schema};

fn main() {
    let source = SourceFile::new(
        "schema.lang",
        "record User { manager: Manager, id: int } record Manager { user: User }",
    );
    let parsed = parse_schema(&source);
    let analysis = analyze(&source, &parsed.tree);

    for diagnostic in parsed.diagnostics.iter().chain(&analysis.diagnostics) {
        eprintln!("{}: {}", diagnostic.code, diagnostic.message);
    }
    if parsed.is_valid() && analysis.is_valid() {
        println!("valid schema: {} record(s)", analysis.symbols.len());
    }
}
