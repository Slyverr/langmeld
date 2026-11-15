use langmeld_core::SourceFile;
use langmeld_declarative_functions_example::lsp::backend;
use langmeld_lsp::LanguageBackend;

#[test]
fn server_backend_publishes_host_semantic_diagnostics() {
    let backend = backend().expect("the checked-in definition should load");
    let source = SourceFile::new(
        "invalid.functionflow",
        "program { fn main() -> i32 { invoke missing(1); } fn main() -> i32 {} }",
    );
    let document = backend.analyze(&source);
    let codes = document
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();

    assert!(backend.capabilities().completion);
    assert!(codes.contains(&"FUNCTION-004"));
    assert!(codes.contains(&"FUNCTION-005"));
}
