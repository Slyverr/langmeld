use langmeld_core::SourceFile;
use language::load;

#[test]
fn runs_a_program_from_the_project_definition() {
    let language = load().expect("starter definition should load");
    assert_eq!(language.specification().name(), {{LANGUAGE_NAME}});

    let source = SourceFile::new(
        "valid.widget",
        "program { let message = \"Hello\"; print message; }",
    );

    let output = language.run(&source).expect("valid program should execute");

    assert_eq!(output, "Hello\n");
}

#[test]
fn reports_an_unknown_variable_at_its_reference() {
    let language = load().expect("starter definition should load");
    let source = SourceFile::new("unknown.widget", "program { print missing; }");

    let error = language
        .run(&source)
        .expect_err("unknown variables should be diagnosed");

    assert!(error.contains("unknown.widget:1:17"), "{error}");
    assert!(error.contains("unknown variable `missing`"), "{error}");
}

#[test]
fn reports_syntax_errors_before_running_handlers() {
    let language = load().expect("starter definition should load");
    let source = SourceFile::new("invalid.widget", "program { let message = 42; }");

    let error = language
        .run(&source)
        .expect_err("invalid syntax should be rejected");

    assert!(error.contains("invalid.widget:1:"), "{error}");
    assert!(error.contains("LM-PARSE-"), "{error}");
}

#[test]
fn rejects_a_source_without_a_program_root() {
    let language = load().expect("starter definition should load");
    let source = SourceFile::new("empty.widget", "");

    let error = language
        .run(&source)
        .expect_err("a source must contain a program root");

    assert!(error.contains("program did not produce a root construct"));
}

#[test]
fn reuses_a_loaded_language_for_independent_programs() {
    let language = load().expect("starter definition should load");
    let first = SourceFile::new(
        "first.widget",
        "program { let message = \"First\"; print message; }",
    );
    let second = SourceFile::new("second.widget", "program { print message; }");

    assert_eq!(
        language.run(&first).expect("first program should run"),
        "First\n"
    );

    let error = language
        .run(&second)
        .expect_err("each program should start with fresh runtime state");

    assert!(error.contains("unknown variable `message`"), "{error}");
}
