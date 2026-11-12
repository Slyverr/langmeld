use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use langmeld_project::{
    LanguageSpecLoadError, load_definition_project, load_definition_project_from_manifest,
    load_language_spec,
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "langmeld-definition-project-test-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("test directory should be created");
        Self(path)
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().expect("file has a parent"))
            .expect("parent directory should be created");
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
fn loads_and_parses_a_manifest_backed_definition_project() {
    let project = TestDirectory::new();
    project.write(
        "langmeld.toml",
        "format = 1\ndefinition = \"language/main.langmeld\"\n",
    );
    project.write(
        "language/main.langmeld",
        "language \"Example\" { construct \"item\" { syntax { literal \"item\"; } } }",
    );

    let loaded = load_definition_project(project.path()).unwrap();

    assert!(loaded.is_valid(), "{:?}", loaded.definition().diagnostics);
    assert_eq!(
        loaded.definition().language.as_ref().unwrap().name(),
        "Example"
    );
    assert_eq!(loaded.sources().len(), 1);
    assert_eq!(loaded.manifest().definition, "language/main.langmeld");
}

#[cfg(unix)]
#[test]
fn exposes_watch_paths_for_internal_symlink_imports() {
    let project = TestDirectory::new();
    project.write(
        "langmeld.toml",
        "format = 1\ndefinition = \"language/main.langmeld\"\n",
    );
    project.write(
        "language/main.langmeld",
        "language \"Example\" { import \"../definitions/alias.langmeld\"; }",
    );
    project.write(
        "definitions/actual.langmeld",
        "construct \"item\" { syntax { literal \"item\"; } }",
    );
    std::os::unix::fs::symlink(
        "actual.langmeld",
        project.path().join("definitions/alias.langmeld"),
    )
    .expect("internal definition symlink should be created");

    let loaded = load_definition_project(project.path()).unwrap();

    assert!(loaded.is_valid(), "{:?}", loaded.definition().diagnostics);
    assert!(
        loaded
            .watched_paths()
            .contains(&loaded.root().join("definitions/alias.langmeld"))
    );
    assert!(
        loaded
            .watched_paths()
            .contains(&loaded.root().join("definitions/actual.langmeld"))
    );
    assert!(
        loaded
            .watched_paths()
            .contains(&loaded.root().join("definitions"))
    );
}

#[cfg(unix)]
#[test]
fn avoids_watching_the_project_root_for_top_level_symlink_imports() {
    let project = TestDirectory::new();
    project.write(
        "langmeld.toml",
        "format = 1\ndefinition = \"language/main.langmeld\"\n",
    );
    project.write(
        "language/main.langmeld",
        "language \"Example\" { import \"../definitions/main.langmeld\"; }",
    );
    project.write(
        "definition-sources/main.langmeld",
        "construct \"item\" { syntax { literal \"item\"; } }",
    );
    std::os::unix::fs::symlink("definition-sources", project.path().join("definitions"))
        .expect("top-level definition symlink should be created");

    let loaded = load_definition_project(project.path()).unwrap();

    assert!(loaded.is_valid(), "{:?}", loaded.definition().diagnostics);
    assert!(
        loaded
            .watched_paths()
            .contains(&loaded.root().join("definitions"))
    );
    assert!(
        !loaded
            .watched_paths()
            .iter()
            .any(|path| path == loaded.root())
    );
}

#[test]
fn loads_a_legacy_lm_root_and_reports_a_migration_warning() {
    let project = TestDirectory::new();
    project.write(
        "langmeld.toml",
        "format = 1\ndefinition = \"language/main.lm\"\n",
    );
    project.write(
        "language/main.lm",
        "language \"Example\" { construct \"item\" { syntax { literal \"item\"; } } }",
    );

    let loaded = load_definition_project(project.path()).unwrap();

    assert!(loaded.is_valid(), "{:?}", loaded.definition().diagnostics);
    let warning = loaded
        .definition()
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "LM-DEF-W001")
        .expect("legacy root should produce a migration warning");
    assert!(warning.message.contains("update `definition`"));
    assert_eq!(
        warning
            .location
            .as_ref()
            .map(|location| location.source_name()),
        Some("language/main.lm")
    );
}

#[test]
fn loads_a_valid_language_specification() {
    let project = TestDirectory::new();
    project.write("langmeld.toml", "format = 1\n");
    project.write(
        "language/main.langmeld",
        "language \"Example\" { construct \"item\" { syntax { literal \"item\"; } } }",
    );

    let specification = load_language_spec(project.path()).unwrap();

    assert_eq!(specification.name(), "Example");
}

#[test]
fn returns_definition_diagnostics_with_their_source_snapshots() {
    let project = TestDirectory::new();
    project.write("langmeld.toml", "format = 1\n");
    project.write(
        "language/main.langmeld",
        "language \"Invalid\" { construct \"item\" {} }",
    );

    let error = load_language_spec(project.path()).unwrap_err();

    assert!(matches!(
        &error,
        LanguageSpecLoadError::InvalidDefinition { .. }
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
fn returns_project_errors_without_definition_diagnostics() {
    let project = TestDirectory::new();

    let error = load_language_spec(project.path()).unwrap_err();

    assert!(matches!(&error, LanguageSpecLoadError::Project(_)));
    assert!(error.diagnostics().is_empty());
    assert!(error.sources().is_empty());
}

#[test]
fn preserves_definition_diagnostics_and_imported_sources_for_rendering() {
    let project = TestDirectory::new();
    project.write(
        "langmeld.toml",
        "format = 1\ndefinition = \"language/main.langmeld\"\n",
    );
    project.write(
        "language/main.langmeld",
        "language \"Example\" { import \"constructs/items.langmeld\"; }",
    );
    project.write(
        "language/constructs/items.langmeld",
        "construct \"item\" {}",
    );

    let loaded = load_definition_project(project.path()).unwrap();

    assert!(!loaded.is_valid());
    assert_eq!(loaded.sources().len(), 2);
    assert!(loaded.definition().diagnostics.iter().any(|diagnostic| {
        diagnostic
            .location
            .as_ref()
            .is_some_and(|location| location.source_name() == "language/constructs/items.langmeld")
    }));
}

#[test]
fn loads_a_project_from_a_non_default_manifest_path() {
    let project = TestDirectory::new();
    project.write(
        "config/langmeld.toml",
        "format = 1\ndefinition = \"language/main.langmeld\"\n",
    );
    project.write(
        "config/language/main.langmeld",
        "language \"Example\" { construct \"item\" { syntax { literal \"item\"; } } }",
    );

    let loaded =
        load_definition_project_from_manifest(project.path().join("config/langmeld.toml")).unwrap();

    assert!(loaded.is_valid(), "{:?}", loaded.definition().diagnostics);
    let expected_root = project.path().join("config").canonicalize().unwrap();
    let expected_watched_paths = loaded.watched_paths().to_vec();
    assert_eq!(loaded.root(), expected_root);

    let parts = loaded.into_parts();
    assert_eq!(parts.root, expected_root);
    assert_eq!(parts.manifest.definition, "language/main.langmeld");
    assert!(parts.definition.is_valid());
    assert_eq!(parts.sources.len(), 1);
    assert_eq!(parts.watched_paths, expected_watched_paths);
}
