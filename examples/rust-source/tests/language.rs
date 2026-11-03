use langmeld::{core::{Severity, SourceFile}, language::classify_parse_result_prepared};
use rust_source_language::{RustFunctionSignature, ast, parse_source, parser};

#[test]
fn host_parser_keeps_rust_ast_and_exposes_typed_langmeld_nodes() {
    let source = SourceFile::new(
        "source.rs",
        "// 🧭 UTF-8 before the declaration\n\
         use std::fmt;\n\
         #[inline]\n\
         pub async unsafe fn reconcile<'a, T: Send + Sync>(input: &'a mut T, mode: impl Into<String>) -> Result<(), Box<dyn fmt::Debug>>\n\
         where T: 'a { let _ = mode.into(); Ok(()) }\n\
         fn empty() {}\n",
    );
    let parsed = parse_source(&source).expect("Rust parser callback registers");
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(parsed.program.nodes[0].construct, "rust_file");
    assert_eq!(parsed.program.nodes[0].children.len(), 2);

    let function = &parsed.program.nodes[0].children[0];
    assert_eq!(
        &source.text[function.fields["name"].span.start..function.fields["name"].span.end],
        "reconcile"
    );
    let native = function
        .host_data::<RustFunctionSignature>()
        .expect("host's typed Rust signature projection is retained");
    assert!(native.is_async);
    assert!(native.is_unsafe);
    assert_eq!(native.generics.len(), 2);
    assert_eq!(native.parameters.len(), 2);
    assert_eq!(native.parameters[0].pattern, "input");
    assert_eq!(native.parameters[0].type_text, "& 'a mut T");
    assert_eq!(
        native.return_type.as_deref(),
        Some("Result < () , Box < dyn fmt :: Debug > >")
    );

    let typed = ast::from_program(&parsed.program).expect("typed Langmeld AST converts");
    assert_eq!(typed[0].children.len(), 2);
    let function = ast::Function::try_from(&parsed.program.nodes[0].children[0])
        .expect("generic Rust function converts to the typed model");
    assert_eq!(function.name.value, "reconcile");
    assert!(function.signature.value.contains("reconcile"));
}

#[test]
fn host_parser_reports_malformed_rust_at_a_source_location() {
    let source = SourceFile::new("broken.rs", "// 🧭\npub fn broken( {");
    let parsed = parse_source(&source).expect("parser prepares");
    let diagnostic = parsed
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "LMRS1001")
        .expect("syn parse error is reported");
    assert_eq!(diagnostic.severity, Severity::Error);
    let span = diagnostic.span.expect("source parser supplies a span");
    assert!(span.start <= span.end && span.end <= source.text.len());
    assert!(source.text.is_char_boundary(span.start));
    assert!(source.text.is_char_boundary(span.end));
}

#[test]
fn opaque_rust_regions_do_not_claim_generic_identifier_highlighting() {
    let source = SourceFile::new("source.rs", "fn greet(name: &str) -> String { name.into() }\n");
    let parser = parser().expect("Rust source parser configures");
    let parsed = parser.parse(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert!(classify_parse_result_prepared(parser.language(), &source, &parsed).is_empty());
}
