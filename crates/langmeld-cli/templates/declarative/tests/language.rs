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

    let report = language.run(&source).expect("valid program should execute");

    assert_eq!(report.output, "Hello\n");
    assert!(report.diagnostics.is_empty());
}

#[test]
fn reports_an_unknown_variable_at_its_reference() {
    let language = load().expect("starter definition should load");
    let source = SourceFile::new("unknown.widget", "program { print missing; }");

    let failure = language
        .run(&source)
        .expect_err("unknown variables should be diagnosed");

    assert!(failure.diagnostics.iter().any(|diagnostic| {
        diagnostic.message == "unknown variable `missing`"
            && diagnostic.location.as_ref().is_some_and(|location| {
                location.source_name() == "unknown.widget"
                    && source
                        .text()
                        .get(location.span().start..location.span().end)
                        == Some("missing")
            })
    }));
}

#[test]
fn reports_syntax_errors_before_running_handlers() {
    let language = load().expect("starter definition should load");
    let source = SourceFile::new("invalid.widget", "program { let message = 42; }");

    let failure = language
        .run(&source)
        .expect_err("invalid syntax should be rejected");

    assert!(failure.diagnostics.iter().any(|diagnostic| {
        diagnostic.code.starts_with("LM-PARSE-")
            && diagnostic
                .location
                .as_ref()
                .is_some_and(|location| location.source_name() == "invalid.widget")
    }));
}

#[test]
fn rejects_a_source_without_a_program_root() {
    let language = load().expect("starter definition should load");
    let source = SourceFile::new("empty.widget", "");

    let failure = language
        .run(&source)
        .expect_err("a source must contain a program root");

    assert!(failure.diagnostics.iter().any(|diagnostic| {
        diagnostic.message == "program did not produce a root construct"
            && diagnostic.location.is_some()
    }));
}

#[test]
fn reuses_a_loaded_language_for_independent_programs() {
    let language = load().expect("starter definition should load");
    let first = SourceFile::new(
        "first.widget",
        "program { let message = \"First\"; print message; }",
    );
    let second = SourceFile::new("second.widget", "program { print message; }");

    let report = language.run(&first).expect("first program should run");
    assert_eq!(report.output, "First\n");

    let failure = language
        .run(&second)
        .expect_err("each program should start with fresh runtime state");

    assert!(
        failure
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.message == "unknown variable `message`" })
    );
}

#[test]
fn preserves_output_emitted_before_a_handler_fails() {
    let language = load().expect("starter definition should load");
    let source = SourceFile::new(
        "partial.widget",
        "program { let message = \"Hello\"; print message; print missing; }",
    );

    let failure = language
        .run(&source)
        .expect_err("the unresolved variable should fail the run");

    assert_eq!(failure.partial_output, "Hello\n");
    assert!(
        failure
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == "unknown variable `missing`")
    );
}
