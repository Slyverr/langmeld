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
    project.write("language/main.langmeld", "language \"Example\"");
    project.write("language/constructs/items.langmeld", "construct \"item\"");
    let resolver = DirectoryDefinitionResolver::new(project.path()).unwrap();
    let root = resolver.load("language/main.langmeld").unwrap();

    let imported = resolver
        .resolve(root.key(), "constructs/items.langmeld")
        .unwrap();

    assert_eq!(imported.key(), "language/constructs/items.langmeld");
    assert_eq!(
        resolver
            .sources()
            .iter()
            .map(|source| source.name())
            .collect::<Vec<_>>(),
        [
            "language/constructs/items.langmeld",
            "language/main.langmeld"
        ]
    );
}

#[test]
fn reuses_identity_for_unchanged_files_and_replaces_changed_snapshots() {
    let project = TestDirectory::new();
    project.write("language/main.langmeld", "language \"Example\"");
    let resolver = DirectoryDefinitionResolver::new(project.path()).unwrap();

    let first = resolver.load("language/main.langmeld").unwrap();
    let unchanged = resolver.load("language/main.langmeld").unwrap();

    assert_eq!(first.source().identity(), unchanged.source().identity());

    project.write("language/main.langmeld", "language \"Updated\"");
    let updated = resolver.load("language/main.langmeld").unwrap();

    assert_ne!(first.source().identity(), updated.source().identity());
    assert_eq!(updated.source().text(), "language \"Updated\"");
}

#[test]
fn requires_an_existing_directory_as_project_root() {
    let project = TestDirectory::new();
    project.write("file.langmeld", "language \"Example\"");

    assert!(DirectoryDefinitionResolver::new(project.path().join("file.langmeld")).is_err());
    assert!(DirectoryDefinitionResolver::new(project.path().join("missing")).is_err());
}

#[test]
fn maps_existing_and_not_yet_created_paths_to_project_keys() {
    let project = TestDirectory::new();
    project.write("language/main.langmeld", "language \"Example\"");
    let resolver = DirectoryDefinitionResolver::new(project.path()).unwrap();

    assert_eq!(
        resolver
            .key_for_path(project.path().join("language/main.langmeld"))
            .unwrap(),
        "language/main.langmeld"
    );
    assert_eq!(
        resolver
            .key_for_path(project.path().join("language/constructs/new.langmeld"))
            .unwrap(),
        "language/constructs/new.langmeld"
    );
    assert_eq!(
        resolver
            .key_for_path("language/constructs/other.langmeld")
            .unwrap(),
        "language/constructs/other.langmeld"
    );
}

#[test]
fn rejects_nonexistent_paths_outside_the_project_root() {
    let project = TestDirectory::new();
    let outside = TestDirectory::new();
    let resolver = DirectoryDefinitionResolver::new(project.path()).unwrap();

    let error = resolver
        .key_for_path(outside.path().join("new.langmeld"))
        .unwrap_err();

    assert!(error.to_string().contains("outside the project root"));
}

#[test]
fn imported_definition_diagnostics_keep_the_imported_source_name() {
    let project = TestDirectory::new();
    project.write(
        "language.langmeld",
        "language \"Example\" { import \"constructs/item.langmeld\"; }",
    );
    project.write("constructs/item.langmeld", "construct \"item\" {}");
    let resolver = DirectoryDefinitionResolver::new(project.path()).unwrap();
    let root = resolver.load("language.langmeld").unwrap();

    let result = parse_language_definition_project(root, &resolver);

    assert!(!result.is_valid());
    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .location
            .as_ref()
            .is_some_and(|location| location.source_name() == "constructs/item.langmeld")
    }));
}

#[test]
fn allows_parent_segments_while_target_remains_inside_project() {
    let project = TestDirectory::new();
    project.write("language/main.langmeld", "language \"Example\"");
    project.write("constructs/items.langmeld", "construct \"item\"");
    let resolver = DirectoryDefinitionResolver::new(project.path()).unwrap();
    let root = resolver.load("language/main.langmeld").unwrap();

    let imported = resolver.resolve(root.key(), "../constructs/items.langmeld");

    assert_eq!(imported.unwrap().key(), "constructs/items.langmeld");
}

#[test]
fn rejects_imports_that_escape_the_project_root() {
    let project = TestDirectory::new();
    let outside = TestDirectory::new();
    project.write("language/main.langmeld", "language \"Example\"");
    outside.write("private.langmeld", "construct \"secret\"");
    let resolver = DirectoryDefinitionResolver::new(project.path()).unwrap();
    let root = resolver.load("language/main.langmeld").unwrap();
    let outside_name = outside
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .expect("temporary directory name should be UTF-8");
    let relative = format!("../../{outside_name}/private.langmeld");

    let result = resolver.resolve(root.key(), &relative);

    assert!(result.unwrap_err().contains("outside the project root"));
}

#[test]
fn rejects_absolute_and_non_portable_import_paths() {
    let project = TestDirectory::new();
    project.write("main.langmeld", "language \"Example\"");
    let resolver = DirectoryDefinitionResolver::new(project.path()).unwrap();
    let root = resolver.load("main.langmeld").unwrap();

    assert!(resolver.resolve(root.key(), "/etc/passwd").is_err());
    assert!(resolver.resolve(root.key(), "C:/outside.langmeld").is_err());
    assert!(
        resolver
            .resolve(root.key(), "folder\\items.langmeld")
            .is_err()
    );
    assert!(resolver.load("../main.langmeld").is_err());
}

#[cfg(unix)]
#[test]
fn rejects_symlinks_that_escape_the_project_root() {
    let project = TestDirectory::new();
    let outside = TestDirectory::new();
    project.write("language/main.langmeld", "language \"Example\"");
    outside.write("secret.langmeld", "construct \"secret\"");
    std::os::unix::fs::symlink(
        outside.path().join("secret.langmeld"),
        project.path().join("secret.langmeld"),
    )
    .expect("test symlink should be created");
    let resolver = DirectoryDefinitionResolver::new(project.path()).unwrap();
    let root = resolver.load("language/main.langmeld").unwrap();

    let result = resolver.resolve(root.key(), "../secret.langmeld");

    assert!(result.unwrap_err().contains("outside the project root"));
}

#[cfg(unix)]
#[test]
fn rejects_nonexistent_paths_beneath_symlinks_that_escape_the_project_root() {
    let project = TestDirectory::new();
    let outside = TestDirectory::new();
    std::os::unix::fs::symlink(outside.path(), project.path().join("external"))
        .expect("test symlink should be created");
    let resolver = DirectoryDefinitionResolver::new(project.path()).unwrap();

    let error = resolver
        .key_for_path(project.path().join("external/new.langmeld"))
        .unwrap_err();

    assert!(error.to_string().contains("outside the project root"));
}
