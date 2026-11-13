use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use langmeld_core::SourceFile;
use language::{LanguageLoadError, load, load_from};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        loop {
            let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "langmeld-starter-language-test-{}-{id}",
                std::process::id()
            ));

            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("test project directory should be created: {error}"),
            }
        }
    }

    fn write(&self, relative_path: &str, contents: &str) {
        let path = self.0.join(relative_path);
        fs::create_dir_all(path.parent().expect("test file should have a parent"))
            .expect("test file parent should be created");
        fs::write(path, contents).expect("test file should be written");
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

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
fn loads_a_language_from_an_explicit_project_root() {
    let project = TestDirectory::new();
    project.write("langmeld.toml", "format = 1\n");
    project.write(
        "language/main.langmeld",
        "language \"External\" { construct \"program\" { syntax { literal \"program\"; } } }",
    );

    let language = load_from(project.path()).expect("external project should load");

    assert_eq!(language.specification().name(), "External");
}

#[test]
fn returns_source_backed_diagnostics_for_an_invalid_external_definition() {
    let project = TestDirectory::new();
    project.write("langmeld.toml", "format = 1\n");
    project.write(
        "language/main.langmeld",
        "language \"Invalid\" { construct \"program\" {} }",
    );

    let Err(error) = load_from(project.path()) else {
        panic!("invalid definition should fail to load");
    };

    assert!(matches!(
        &error,
        LanguageLoadError::InvalidDefinition { .. }
    ));
    assert!(!error.diagnostics().is_empty());
    assert_eq!(error.sources().len(), 1);
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic
            .location
            .as_ref()
            .is_some_and(|location| location.source_name() == "language/main.langmeld")
    }));
}

#[test]
fn returns_a_typed_project_error_when_the_manifest_is_missing() {
    let project = TestDirectory::new();

    let Err(error) = load_from(project.path()) else {
        panic!("missing manifest should fail to load");
    };

    assert!(matches!(&error, LanguageLoadError::Project(_)));
    assert!(error.diagnostics().is_empty());
    assert!(error.sources().is_empty());
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
                    && source.slice(location.span()) == Some("missing")
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
