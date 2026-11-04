use aster_language::{
    RuntimeContext, analyze_sources, evaluate, load_language, load_language_runtime, load_project,
    load_project_runtime, parse_source,
};
use langmeld::core::SourceFile;

#[test]
fn project_composes_functions_scopes_and_an_extension() {
    let language = load_language().expect("compiled language model loads");
    let runtime_language = load_language_runtime().expect("runtime language model loads");
    assert_eq!(language.spec(), runtime_language.spec());
    assert!(
        language
            .spec()
            .constructs
            .iter()
            .any(|item| item.id == "assert")
    );

    let compiled_project = load_project().expect("compiled project loads");
    let runtime_project = load_project_runtime().expect("runtime project loads");
    assert_eq!(
        compiled_project.language.spec(),
        runtime_project.language.spec()
    );

    let source = SourceFile::new(
        "hello.aster.lmprog",
        include_str!("../examples/hello.aster.lmprog"),
    );
    let parsed = parse_source(&source).expect("parser prepares");
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(parsed.program.nodes[0].children.len(), 2);
    assert_eq!(parsed.program.nodes[0].children[0].construct, "function");

    let programs = vec![parsed.program];
    let analysis =
        analyze_sources(&language, &[source.clone()], &[&programs[0]]).expect("host analysis runs");
    assert!(
        analysis.diagnostics.is_empty(),
        "{:#?}",
        analysis.diagnostics
    );
    assert_eq!(analysis.resolved_references.len(), 4);
    assert!(
        analysis
            .host_type_facts
            .iter()
            .any(|fact| fact.type_name == "number")
    );

    let output = evaluate(&RuntimeContext {
        language: &language,
        sources: &[source],
        programs: &programs,
        analysis: &analysis,
    })
    .expect("host lowers, optimizes, and interprets the program");
    assert_eq!(output, ["value = 21", "answer = 42"]);
}

#[test]
fn generic_analysis_resolves_parameter_references_in_function_scopes() {
    let language = load_language().expect("compiled language model loads");
    let source = SourceFile::new(
        "scopes.aster.lmprog",
        "program {\n  fn first {\n    param value\n    print value\n    return value;\n  }\n  fn second {\n    param value\n    print value\n    return value;\n  }\n}",
    );
    let parsed = parse_source(&source).expect("parser prepares");
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let analysis =
        analyze_sources(&language, &[source], &[&parsed.program]).expect("host analysis runs");
    assert!(
        analysis.diagnostics.is_empty(),
        "{:#?}",
        analysis.diagnostics
    );
    assert_eq!(analysis.resolved_references.len(), 2);
}

#[test]
fn duplicate_function_candidates_report_an_ambiguous_call() {
    let language = load_language().expect("compiled language model loads");
    let source = SourceFile::new(
        "ambiguous.aster.lmprog",
        "program {\n  fn identity {\n    param value\n    return value;\n  }\n  fn identity {\n    param value\n    return value;\n  }\n  fn main {\n    call identity with [1] as result;\n    return result;\n  }\n}",
    );
    let parsed = parse_source(&source).expect("parser prepares");
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let analysis =
        analyze_sources(&language, &[source], &[&parsed.program]).expect("host analysis runs");
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|finding| finding.diagnostic.code == "LM2006"),
        "{:#?}",
        analysis.diagnostics
    );
    assert!(
        analysis
            .resolved_references
            .iter()
            .all(|reference| reference.kind != "function")
    );
}

#[test]
fn incomplete_expressions_keep_source_diagnostics() {
    let source = SourceFile::new(
        "incomplete.aster.lmprog",
        "program {\n  fn main {\n    return 1 +;\n  }\n}",
    );
    let parsed = parse_source(&source).expect("parser prepares");
    assert!(!parsed.diagnostics.is_empty());
}

#[test]
fn expressions_follow_precedence_associativity_and_nested_groups() {
    let language = load_language().expect("compiled language model loads");
    let source = SourceFile::new(
        "expressions.aster.lmprog",
        "program {\n  fn main {\n    let precedence = 2 + 3 * 4;\n    let associativity = 10 - 3 - 2;\n    let grouped = (2 + 3) * 4;\n    print precedence\n    print associativity\n    print grouped\n    return grouped;\n  }\n}",
    );
    let parsed = parse_source(&source).expect("parser prepares");
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let programs = vec![parsed.program];
    let analysis =
        analyze_sources(&language, &[source.clone()], &[&programs[0]]).expect("host analysis runs");
    assert!(
        analysis.diagnostics.is_empty(),
        "{:#?}",
        analysis.diagnostics
    );

    let output = evaluate(&RuntimeContext {
        language: &language,
        sources: &[source],
        programs: &programs,
        analysis: &analysis,
    })
    .expect("host lowers, optimizes, and interprets each expression");
    assert_eq!(
        output,
        ["precedence = 14", "associativity = 5", "grouped = 20"]
    );
}

#[test]
fn host_type_analysis_checks_expression_names_call_arity_and_return_values() {
    let language = load_language().expect("compiled language model loads");
    let source = SourceFile::new(
        "invalid.aster.lmprog",
        "program {\n  fn identity {\n    param value\n    return missing;\n  }\n  call identity with [] as result;\n}",
    );
    let parsed = parse_source(&source).expect("parser prepares");
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let analysis =
        analyze_sources(&language, &[source], &[&parsed.program]).expect("host analysis runs");
    let codes = analysis
        .diagnostics
        .iter()
        .map(|finding| finding.diagnostic.code.as_str())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"ASTER1006"), "{codes:?}");
    assert!(codes.contains(&"ASTER1005"), "{codes:?}");
}

#[test]
fn host_runtime_executes_the_extension_handler_and_reports_failed_assertions() {
    let language = load_language().expect("compiled language model loads");
    let source = SourceFile::new(
        "assertion.aster.lmprog",
        "program {\n  fn main {\n    let answer = 41 + 1;\n    assert answer equals 41;\n    return answer;\n  }\n}",
    );
    let parsed = parse_source(&source).expect("parser prepares");
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let programs = vec![parsed.program];
    let analysis =
        analyze_sources(&language, &[source.clone()], &[&programs[0]]).expect("host analysis runs");
    assert!(
        analysis.diagnostics.is_empty(),
        "{:#?}",
        analysis.diagnostics
    );

    let error = evaluate(&RuntimeContext {
        language: &language,
        sources: &[source],
        programs: &programs,
        analysis: &analysis,
    })
    .expect_err("the extension assertion rejects the incorrect value");
    assert!(error.contains("assertion failed: `answer` is 42, expected 41"));
}
