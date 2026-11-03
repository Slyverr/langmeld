use langmeld::core::SourceFile;
use ledger_language::parse_source;

fn main() {
    let source = SourceFile::new("hello.ledger.lmprog", include_str!("hello.ledger.lmprog"));
    let parsed = parse_source(&source).expect("starter language definition loads");
    for diagnostic in &parsed.diagnostics {
        eprintln!("{}: {}", diagnostic.code, diagnostic.message);
    }
    assert!(
        parsed.diagnostics.is_empty(),
        "starter program has parse errors"
    );
    println!(
        "Parsed {} top-level construct(s).",
        parsed.program.nodes.len()
    );
}
