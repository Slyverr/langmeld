use langmeld_core::SourceFile;
use language::{load, run};

#[test]
fn runs_a_program_from_the_project_definition() {
    let language = load().expect("starter definition should load");
    assert_eq!(language.name(), {{LANGUAGE_NAME}});

    let source = SourceFile::new(
        "valid.widget",
        "program { let message = \"Hello\"; print message; }",
    );

    let output = run(&source).expect("valid program should execute");

    assert_eq!(output, "Hello\n");
}

#[test]
fn reports_an_unknown_variable_at_its_reference() {
    let source = SourceFile::new("unknown.widget", "program { print missing; }");

    let error = run(&source).expect_err("unknown variables should be diagnosed");

    assert!(error.contains("unknown.widget:1:17"), "{error}");
    assert!(error.contains("unknown variable `missing`"), "{error}");
}

#[test]
fn reports_syntax_errors_before_running_handlers() {
    let source = SourceFile::new("invalid.widget", "program { let message = 42; }");

    let error = run(&source).expect_err("invalid syntax should be rejected");

    assert!(error.contains("invalid.widget:1:"), "{error}");
    assert!(error.contains("LM-PARSE-"), "{error}");
}

#[test]
fn rejects_a_source_without_a_program_root() {
    let source = SourceFile::new("empty.widget", "");

    let error = run(&source).expect_err("a source must contain a program root");

    assert!(error.contains("program did not produce a root construct"));
}
