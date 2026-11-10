use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use langmeld_language::{DefinitionResolver, parse_language_definition_project};
use langmeld_project::DirectoryDefinitionResolver;

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("langmeld-project-test-{}-{id}", std::process::id()));
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
fn resolves_nested_imports_and_retains_source_snapshots() {
    let project = TestDirectory::new();
    project.write("language/main.lm", "language \"Example\"");
    project.write("language/constructs/items.lm", "construct \"item\"");
    let resolver = DirectoryDefinitionResolver::new(project.path()).unwrap();
    let root = resolver.load("language/main.lm").unwrap();

    let imported = resolver.resolve(&root.key, "constructs/items.lm").unwrap();

    assert_eq!(imported.key, "language/constructs/items.lm");
    assert_eq!(
        resolver
            .sources()
            .iter()
            .map(|source| source.name.as_str())
            .collect::<Vec<_>>(),
        ["language/constructs/items.lm", "language/main.lm"]
    );
}

#[test]
fn requires_an_existing_directory_as_project_root() {
    let project = TestDirectory::new();
    project.write("file.lm", "language \"Example\"");

    assert!(DirectoryDefinitionResolver::new(project.path().join("file.lm")).is_err());
    assert!(DirectoryDefinitionResolver::new(project.path().join("missing")).is_err());
}

#[test]
fn imported_definition_diagnostics_keep_the_imported_source_name() {
    let project = TestDirectory::new();
    project.write(
        "language.lm",
        "language \"Example\" { import \"constructs/item.lm\"; }",
    );
    project.write("constructs/item.lm", "construct \"item\" {}");
    let resolver = DirectoryDefinitionResolver::new(project.path()).unwrap();
    let root = resolver.load("language.lm").unwrap();

    let result = parse_language_definition_project(root, &resolver);

    assert!(!result.is_valid());
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .location
            .as_ref()
            .is_some_and(|location| location.source_name == "constructs/item.lm")
    }));
}

#[test]
fn allows_parent_segments_while_target_remains_inside_project() {
    let project = TestDirectory::new();
    project.write("language/main.lm", "language \"Example\"");
    project.write("constructs/items.lm", "construct \"item\"");
    let resolver = DirectoryDefinitionResolver::new(project.path()).unwrap();
    let root = resolver.load("language/main.lm").unwrap();

    let imported = resolver.resolve(&root.key, "../constructs/items.lm");

    assert_eq!(imported.unwrap().key, "constructs/items.lm");
}

#[test]
fn rejects_imports_that_escape_the_project_root() {
    let project = TestDirectory::new();
    let outside = TestDirectory::new();
    project.write("language/main.lm", "language \"Example\"");
    outside.write("private.lm", "construct \"secret\"");
    let resolver = DirectoryDefinitionResolver::new(project.path()).unwrap();
    let root = resolver.load("language/main.lm").unwrap();
    let outside_name = outside
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .expect("temporary directory name should be UTF-8");
    let relative = format!("../../{outside_name}/private.lm");

    let result = resolver.resolve(&root.key, &relative);

    assert!(result.unwrap_err().contains("outside the project root"));
}

#[test]
fn rejects_absolute_and_non_portable_import_paths() {
    let project = TestDirectory::new();
    project.write("main.lm", "language \"Example\"");
    let resolver = DirectoryDefinitionResolver::new(project.path()).unwrap();
    let root = resolver.load("main.lm").unwrap();

    assert!(resolver.resolve(&root.key, "/etc/passwd").is_err());
    assert!(resolver.resolve(&root.key, "C:/outside.lm").is_err());
    assert!(resolver.resolve(&root.key, "folder\\items.lm").is_err());
    assert!(resolver.load("../main.lm").is_err());
}

#[cfg(unix)]
#[test]
fn rejects_symlinks_that_escape_the_project_root() {
    let project = TestDirectory::new();
    let outside = TestDirectory::new();
    project.write("language/main.lm", "language \"Example\"");
    outside.write("secret.lm", "construct \"secret\"");
    std::os::unix::fs::symlink(
        outside.path().join("secret.lm"),
        project.path().join("secret.lm"),
    )
    .expect("test symlink should be created");
    let resolver = DirectoryDefinitionResolver::new(project.path()).unwrap();
    let root = resolver.load("language/main.lm").unwrap();

    let result = resolver.resolve(&root.key, "../secret.lm");

    assert!(result.unwrap_err().contains("outside the project root"));
}
