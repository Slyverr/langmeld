use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use langmeld_project::{load_definition_project, load_definition_project_from_manifest};

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
    assert_eq!(loaded.root(), expected_root);

    let (root, manifest, definition, sources) = loaded.into_parts();
    assert_eq!(root, expected_root);
    assert_eq!(manifest.definition, "language/main.langmeld");
    assert!(definition.is_valid());
    assert_eq!(sources.len(), 1);
}
