use langmeld_core::SourceFile;
use langmeld_declarative_functions_example::{analyze, diagnostics};

#[test]
fn accepts_parameterized_functions_and_forward_calls() {
    let source = SourceFile::new(
        "forward.functionflow",
        "program { fn main() -> i32 { invoke add(20, 22); } fn add(left: i32, right: i32) -> i32 {} }",
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
    assert_eq!(analysis.parsed.tree().functions[1].parameters.len(), 2);
    assert_eq!(
        analysis.parsed.tree().functions[1].parameters[0].name,
        "left"
    );
    assert_eq!(
        analysis.parsed.tree().functions[1].parameters[1].name,
        "right"
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
