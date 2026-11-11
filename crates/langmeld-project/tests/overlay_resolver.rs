use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use langmeld_core::SourceFile;
use langmeld_language::parse_language_definition_project;
use langmeld_project::{InMemoryDefinitionResolver, OverlayDefinitionResolver};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "langmeld-overlay-resolver-test-{}-{id}",
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
fn combines_local_and_embedded_construct_modules() {
    let project = TestDirectory::new();
    project.write(
        "language/main.langmeld",
        r#"language "Geometry" {
            import "constructs/program.langmeld";
            import "extensions/geometry/drawing.langmeld";
            root "program";
        }"#,
    );
    project.write(
        "language/constructs/program.langmeld",
        r#"construct "program" {
            syntax { literal "program"; }
            block "{" "}" { allow "draw"; }
        }"#,
    );
    project.write(
        "language/extensions/geometry/drawing.langmeld",
        r#"construct "disk_draw" { syntax { literal "invalid"; } }"#,
    );
    let mut bundled = InMemoryDefinitionResolver::new();
    bundled
        .insert(
            "language/extensions/geometry/drawing.langmeld",
            SourceFile::new(
                "language/extensions/geometry/drawing.langmeld",
                r#"construct "draw" {
                    syntax {
                        literal "draw";
                        choice "shape" { value "circle"; value "rectangle"; }
                        literal ";";
                    }
                }"#,
            ),
        )
        .unwrap();
    let resolver = OverlayDefinitionResolver::new(project.path(), bundled).unwrap();
    let root = resolver.load("language/main.langmeld").unwrap();

    let result = parse_language_definition_project(root, &resolver);

    assert!(result.is_valid(), "{:?}", result.diagnostics);
    let sources = resolver.sources();
    assert_eq!(sources.len(), 3);
    assert!(sources.iter().any(|source| {
        source.name() == "language/extensions/geometry/drawing.langmeld"
            && source.text().contains("construct \"draw\"")
    }));
}

#[test]
fn rejects_imports_that_escape_the_project_root_before_overlay_lookup() {
    let project = TestDirectory::new();
    project.write("language/main.langmeld", "language \"Example\"");
    let resolver =
        OverlayDefinitionResolver::new(project.path(), InMemoryDefinitionResolver::new()).unwrap();
    let root = resolver.load("language/main.langmeld").unwrap();

    let error = langmeld_language::DefinitionResolver::resolve(
        &resolver,
        root.key(),
        "../../outside.langmeld",
    )
    .unwrap_err();

    assert!(error.contains("outside the project root"));
}

#[test]
fn resolves_new_imports_from_the_overlay_before_reading_disk() {
    let project = TestDirectory::new();
    project.write(
        "language/main.langmeld",
        r#"language "Example" {
            import "constructs/new.langmeld";
            root "program";
        }"#,
    );
    let mut overlay = InMemoryDefinitionResolver::new();
    overlay
        .insert(
            "language/constructs/new.langmeld",
            SourceFile::new(
                "file:///workspace/language/constructs/new.langmeld",
                r#"construct "program" { syntax { literal "program"; } }"#,
            ),
        )
        .unwrap();
    let resolver = OverlayDefinitionResolver::new(project.path(), overlay).unwrap();
    let root = resolver.load("language/main.langmeld").unwrap();

    let result = parse_language_definition_project(root, &resolver);

    assert!(result.is_valid(), "{:?}", result.diagnostics);
    assert!(
        resolver.sources().iter().any(|source| {
            source.name() == "file:///workspace/language/constructs/new.langmeld"
        })
    );
}

#[test]
fn loads_an_open_root_definition_from_the_overlay() {
    let project = TestDirectory::new();
    project.write("language/main.langmeld", "language \"Disk version\" {");
    let mut overlay = InMemoryDefinitionResolver::new();
    overlay
        .insert(
            "language/main.langmeld",
            SourceFile::new(
                "file:///workspace/language/main.langmeld",
                "language \"Open version\"",
            ),
        )
        .unwrap();
    let resolver = OverlayDefinitionResolver::new(project.path(), overlay).unwrap();

    let root = resolver.load("language/main.langmeld").unwrap();

    assert_eq!(root.source().text(), "language \"Open version\"");
    assert_eq!(
        root.source().name(),
        "file:///workspace/language/main.langmeld"
    );
}
