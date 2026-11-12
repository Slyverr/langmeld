use langmeld_core::SourceFile;
use langmeld_declarative_functions_example::{analyze, diagnostics, generated::constructs};
use langmeld_language::{TypedConstruct, parse};
use langmeld_project::load_language_spec;

#[test]
fn accepts_parameterized_functions_and_forward_calls() {
    let source = SourceFile::new(
        "forward.functionflow",
        "program { fn main() -> i32 { invoke add(20, 22); call add(20, 22); } fn add(left: i32, right: i32) -> i32 {} }",
    );
    let analysis = analyze(&source).expect("the checked-in grammar should load");

    assert!(
        analysis.parsed.is_valid(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.semantic.is_valid(),
        "{:?}",
        analysis.semantic.diagnostics()
    );
    assert_eq!(analysis.parsed.tree().functions.len(), 2);
    assert_eq!(analysis.parsed.tree().functions[0].calls.len(), 2);
    assert_eq!(analysis.parsed.tree().functions[0].calls[0].name, "add");
    assert_eq!(analysis.parsed.tree().functions[0].calls[1].name, "add");
    assert_eq!(analysis.parsed.tree().functions[1].parameters.len(), 2);
    assert_eq!(
        analysis.parsed.tree().functions[1].parameters[0].name,
        "left"
    );
    assert_eq!(
        analysis.parsed.tree().functions[1].parameters[1].name,
        "right"
    );
    assert_eq!(
        analysis.parsed.tree().functions[1].parameters[0].value_type,
        "i32"
    );
    assert_eq!(
        analysis.parsed.tree().functions[1].parameters[1].value_type,
        "i32"
    );
}

#[test]
fn reports_duplicate_functions_and_unknown_calls() {
    let source = SourceFile::new(
        "invalid.functionflow",
        "program { fn main() -> i32 { invoke missing(1); } fn main() -> i32 {} }",
    );
    let analysis = analyze(&source).expect("the checked-in grammar should load");
    let diagnostics = diagnostics(&analysis);
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();

    assert!(codes.contains(&"FUNCTION-004"));
    assert!(codes.contains(&"FUNCTION-005"));
}

#[test]
fn reports_call_arity_mismatches() {
    let source = SourceFile::new(
        "arity.functionflow",
        "program { fn main() -> i32 { invoke add(1); } fn add(left: i32, right: i32) -> i32 {} }",
    );
    let analysis = analyze(&source).expect("the checked-in grammar should load");
    let diagnostics = diagnostics(&analysis);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "FUNCTION-006"
            && diagnostic.message.contains("expects 2 argument(s), got 1")
    }));
}

#[test]
fn requires_commas_between_function_parameters() {
    let source = SourceFile::new(
        "missing-comma.functionflow",
        "program { fn add(left: i32 right: i32) -> i32 {} }",
    );
    let analysis = analyze(&source).expect("the checked-in grammar should load");

    assert!(!analysis.parsed.is_valid());
}

#[test]
fn rejects_trailing_commas_in_function_parameters() {
    let source = SourceFile::new(
        "trailing-comma.functionflow",
        "program { fn add(left: i32,) -> i32 {} }",
    );
    let analysis = analyze(&source).expect("the checked-in grammar should load");

    assert!(!analysis.parsed.is_valid());
}

#[test]
fn generated_views_reject_mismatched_sources_and_parse_results() {
    let source = SourceFile::new("typed.functionflow", "program { fn main() -> i32 {} }");
    let language = load_language_spec(env!("CARGO_MANIFEST_DIR"))
        .expect("the checked-in definition project should load");
    let parsed = parse(&source, &language);
    let function = &parsed.constructs()[0].children[0];
    let view = constructs::Function::from_construct(&source, &parsed, function)
        .expect("the generated view should accept its source construct");

    assert_eq!(view.name().expect("name is captured").text, "main");

    let same_text = SourceFile::new("typed-copy.functionflow", source.text());
    let source_error = constructs::Function::from_construct(&same_text, &parsed, function)
        .expect_err("a different source snapshot must not be accepted");
    assert_eq!(source_error.code, "LM-TYPED-001");

    let other_parse = parse(&same_text, &language);
    let membership_error = constructs::Function::from_construct(&same_text, &other_parse, function)
        .expect_err("a construct from another parse result must not be accepted");
    assert_eq!(membership_error.code, "LM-TYPED-001");
}
