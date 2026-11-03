use csignatures_language::parse_source;
use langmeld::core::SourceFile;

fn main() {
    let source = SourceFile::new("math.c", include_str!("math.c"));
    let parsed = parse_source(&source).expect("starter language definition loads");
    for diagnostic in &parsed.diagnostics {
        eprintln!("{}: {}", diagnostic.code, diagnostic.message);
    }
    assert!(
        parsed.diagnostics.is_empty(),
        "starter program has parse errors"
    );
    println!(
        "Parsed {} C prototype(s).",
        parsed.program.nodes[0].children.len()
    );
}
