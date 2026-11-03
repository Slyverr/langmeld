use langmeld::core::SourceFile;
use ledger_language::{
    RuntimeContext, evaluate, load_language, load_language_runtime, load_project,
    load_project_runtime, parse_source,
};

#[test]
fn definition_and_example_parse_and_analyze() {
    let language = load_language().expect("compiled language model loads");
    assert_eq!(language.spec().name, "Ledger");
    let runtime_language = load_language_runtime().expect("runtime language model loads");
    assert_eq!(language.spec(), runtime_language.spec());
    let compiled_project = load_project().expect("compiled project model loads");
    let runtime_project = load_project_runtime().expect("runtime project model loads");
    assert_eq!(
        compiled_project.language.spec(),
        runtime_project.language.spec()
    );

    let source = SourceFile::new(
        "hello.ledger.lmprog",
        include_str!("../examples/hello.ledger.lmprog"),
    );
    let parsed = parse_source(&source).expect("parser prepares");
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(parsed.program.nodes[0].construct, "program");
    let root = &parsed.program.nodes[0];
    assert_eq!(root.children.len(), 6);
    assert_eq!(root.children[0].construct, "set");
    assert_eq!(root.children[2].construct, "add");
    assert_eq!(root.children[3].construct, "subtract");
    assert_eq!(root.children[4].construct, "print");
    assert_eq!(root.children[5].construct, "print");

    let sources = vec![source];
    let programs = vec![parsed.program];
    let analysis = ledger_language::analyze_sources(&language, &sources, &[&programs[0]])
        .expect("workspace analysis runs");
    assert!(
        analysis.diagnostics.is_empty(),
        "{:#?}",
        analysis.diagnostics
    );
    assert_eq!(analysis.resolved_references.len(), 6);

    let output = evaluate(&RuntimeContext {
        language: &language,
        sources: &sources,
        programs: &programs,
        analysis: &analysis,
    })
    .expect("the host interpreter evaluates the program");
    assert_eq!(output, ["total = 42", "remainder = 23"]);
}

#[test]
fn interpreter_enforces_execution_order_after_workspace_resolution() {
    let language = load_language().expect("compiled language model loads");
    let source = SourceFile::new(
        "ordered.ledger.lmprog",
        "program {\n  print later\n  set later to 5\n}\n",
    );
    let parsed = parse_source(&source).expect("parser prepares");
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let sources = vec![source];
    let programs = vec![parsed.program];
    let analysis = ledger_language::analyze_sources(&language, &sources, &[&programs[0]])
        .expect("workspace analysis runs");
    assert!(
        analysis.diagnostics.is_empty(),
        "{:#?}",
        analysis.diagnostics
    );
    assert_eq!(analysis.resolved_references.len(), 1);

    let error = evaluate(&RuntimeContext {
        language: &language,
        sources: &sources,
        programs: &programs,
        analysis: &analysis,
    })
    .expect_err("execution before declaration is rejected");
    assert!(error.contains("variable `later` is used before it is set"));
}
