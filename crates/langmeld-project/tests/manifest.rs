use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use langmeld_project::{
    DEFAULT_DEFINITION_PATH, PROJECT_MANIFEST_FILE, ProjectManifest, ProjectManifestError,
    RustBindings, read_project_manifest,
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "langmeld-manifest-test-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("test directory should be created");
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn parses_the_root_definition_from_a_supported_manifest() {
    let manifest =
        ProjectManifest::parse("format = 1\ndefinition = \"language/language.langmeld\"\n")
            .unwrap();

    assert_eq!(manifest.definition, "language/language.langmeld");
}

#[test]
fn defaults_to_the_conventional_main_definition() {
    let manifest = ProjectManifest::parse("format = 1\n").unwrap();

    assert_eq!(manifest.definition, DEFAULT_DEFINITION_PATH);
    assert_eq!(manifest.rust_bindings, RustBindings::Dynamic);
}

#[test]
fn parses_typed_rust_bindings_and_rejects_unknown_modes() {
    let manifest = ProjectManifest::parse("format = 1\nrust_bindings = \"typed\"\n").unwrap();

    assert_eq!(manifest.rust_bindings, RustBindings::Typed);

    let invalid = ProjectManifest::parse("format = 1\nrust_bindings = \"generated\"\n");
    assert!(matches!(
        invalid,
        Err(ProjectManifestError::InvalidRustBindings)
    ));
}

#[test]
fn rejects_unsupported_formats_and_unknown_fields() {
    let version = ProjectManifest::parse("format = 2\ndefinition = \"language.langmeld\"\n");
    assert!(matches!(
        version,
        Err(ProjectManifestError::UnsupportedFormat(2))
    ));

    let unknown = ProjectManifest::parse(
        "format = 1\ndefinition = \"language.langmeld\"\nlangauge = \"typo\"\n",
    );
    assert!(matches!(
        unknown,
        Err(ProjectManifestError::UnknownField(_))
    ));

    let invalid_definition_type = ProjectManifest::parse("format = 1\ndefinition = 42\n");
    assert!(matches!(
        invalid_definition_type,
        Err(ProjectManifestError::MissingOrInvalidField("definition"))
    ));
}

#[test]
fn rejects_invalid_or_non_portable_definition_paths() {
    for path in [
        "",
        "language.txt",
        "language.lm",
        "/absolute/language.langmeld",
        "../language.langmeld",
        "C:/language.langmeld",
        "language\\root.langmeld",
    ] {
        let manifest = format!("format = 1\ndefinition = {path:?}\n");
        assert!(
            matches!(
                ProjectManifest::parse(&manifest),
                Err(ProjectManifestError::InvalidDefinitionPath(_))
            ),
            "accepted invalid definition path {path:?}"
        );
    }
}

#[test]
fn reads_the_conventional_manifest_from_a_project_root() {
    let project = TestDirectory::new();
    fs::write(project.0.join(PROJECT_MANIFEST_FILE), "format = 1\n")
        .expect("manifest should be written");

    let manifest = read_project_manifest(&project.0).unwrap();

    assert_eq!(manifest.definition, DEFAULT_DEFINITION_PATH);
}
