use langmeld_aster_language_example::{Statement, SymbolKind, ValueType, analyze, parse_program};
use langmeld_core::{SourceDatabase, SourceFile};

#[test]
fn resolves_forward_calls_and_scoped_local_references() {
    let source = SourceFile::new(
        "main.aster",
        r#"fn main() -> i32 {
            let value = twice(20);
            {
                let value = value + 2;
                return value;
            }
        }
        fn twice(input: i32) -> i32 {
            return input * 2;
        }"#,
    );

    let parsed = parse_program(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let analysis = analyze(&source, &parsed.tree);

    assert!(analysis.is_valid(), "{:?}", analysis.diagnostics);
    let call = analysis
        .references
        .iter()
        .find(|reference| reference.name == "twice")
        .expect("forward function reference");
    assert_eq!(
        analysis.symbols[call.target.expect("resolved function")].kind,
        SymbolKind::Function
    );
    let values = analysis
        .symbols
        .iter()
        .filter(|symbol| symbol.name == "value")
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 2);
    assert_ne!(values[0].scope, values[1].scope);
    assert!(
        analysis
            .references
            .iter()
            .all(|reference| reference.target.is_some())
    );
}

#[test]
fn reports_unresolved_names_and_same_scope_duplicates() {
    let source = SourceFile::new(
        "invalid.aster",
        "fn main() -> i32 { let value = missing; let value = 2; let before = later; let later = 1; return later; }",
    );
    let parsed = parse_program(&source);
    let analysis = analyze(&source, &parsed.tree);

    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == "ASTER-SEM-002")
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == "ASTER-SEM-011")
    );
    assert!(
        analysis
            .references
            .iter()
            .any(|reference| { reference.name == "missing" && reference.target.is_none() })
    );
    let later_references = analysis
        .references
        .iter()
        .filter(|reference| reference.name == "later")
        .collect::<Vec<_>>();
    assert_eq!(later_references.len(), 2);
    assert!(later_references[0].target.is_none());
    assert!(later_references[1].target.is_some());
}

#[test]
fn checks_call_arity_argument_types_and_return_types() {
    let source = SourceFile::new(
        "types.aster",
        r#"fn add(left: i32, right: i32) -> i32 { return left + right; }
        fn main() -> bool { return add("wrong") == 1; }"#,
    );
    let parsed = parse_program(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let analysis = analyze(&source, &parsed.tree);

    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == "ASTER-SEM-009")
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == "ASTER-SEM-010")
    );
    assert!(
        !analysis
            .diagnostics
            .iter()
            .any(|d| d.code == "ASTER-SEM-003")
    );
}

#[test]
fn parses_nested_blocks_as_distinct_ast_nodes() {
    let source = SourceFile::new("blocks.aster", "fn main() -> i32 { { return 1; } }");
    let parsed = parse_program(&source);

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert!(matches!(
        parsed.tree.functions[0].body[0],
        Statement::Block { .. }
    ));
    assert_eq!(parsed.tree.functions[0].return_type, ValueType::I32);
}

#[test]
fn analysis_keeps_the_source_revision_it_describes() {
    let mut sources = SourceDatabase::default();
    let id = sources
        .insert("main.aster", "fn main() -> i32 { return 1; }")
        .unwrap();
    let source = sources.get(id).unwrap();
    let parsed = parse_program(source);

    let analysis = analyze(source, &parsed.tree);

    assert_eq!(analysis.source, parsed.source);
}

#[test]
fn reports_values_that_shadow_functions_when_called() {
    let source = SourceFile::new(
        "shadow.aster",
        "fn value() -> i32 { return 1; } fn main() -> i32 { let value = 2; return value(); }",
    );
    let parsed = parse_program(&source);
    let analysis = analyze(&source, &parsed.tree);

    let reference = analysis
        .references
        .iter()
        .find(|reference| {
            reference.name == "value"
                && reference
                    .target
                    .is_some_and(|target| analysis.symbols[target].kind == SymbolKind::Variable)
        })
        .expect("call through the shadowing local");
    assert!(reference.target.is_some());
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == "ASTER-SEM-007")
    );
}

#[test]
fn keeps_function_navigation_for_a_function_used_as_a_value() {
    let source = SourceFile::new(
        "function-value.aster",
        "fn value() -> i32 { return 1; } fn main() -> i32 { return value; }",
    );
    let parsed = parse_program(&source);
    let analysis = analyze(&source, &parsed.tree);

    let reference = analysis
        .references
        .iter()
        .find(|reference| reference.name == "value")
        .expect("function name reference");
    assert_eq!(
        analysis.symbols[reference.target.expect("declaration target")].kind,
        SymbolKind::Function
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == "ASTER-SEM-002")
    );
}

#[test]
fn requires_functions_to_return_in_the_current_language_subset() {
    let source = SourceFile::new(
        "missing-return.aster",
        "fn main() -> i32 { let value = 1; }",
    );
    let parsed = parse_program(&source);
    let analysis = analyze(&source, &parsed.tree);

    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == "ASTER-SEM-012")
    );
}

#[test]
fn recovers_after_a_malformed_function_and_parses_later_declarations() {
    let source = SourceFile::new(
        "recovery.aster",
        "fn broken( -> i32 { return 0; } fn good() -> i32 { return 1; }",
    );

    let parsed = parse_program(&source);

    assert!(!parsed.diagnostics.is_empty());
    assert!(
        parsed
            .tree
            .functions
            .iter()
            .any(|function| function.name == "good")
    );
}

#[test]
fn bounds_nested_block_parsing() {
    let mut text = String::from("fn main() -> i32 {");
    text.push_str(&"{".repeat(130));
    text.push_str("return 1;");
    text.push_str(&"}".repeat(131));
    let source = SourceFile::new("deep.aster", text);

    let parsed = parse_program(&source);

    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "ASTER-PARSE-014")
    );
}
