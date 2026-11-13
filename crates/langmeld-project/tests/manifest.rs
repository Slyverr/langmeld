mod common;

use langmeld_project::{
    DEFAULT_DEFINITION_PATH, PROJECT_MANIFEST_FILE, ProjectManifest, ProjectManifestError,
    RustBindings, read_project_manifest,
};

use common::TestDirectory;

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
        "/absolute/language.langmeld",
        "../language.langmeld",
        "C:/language.langmeld",
        "language/stream:root.langmeld",
        "language/root<.langmeld",
        "language/root>.langmeld",
        "language/root\".langmeld",
        "language/root|.langmeld",
        "language/root?.langmeld",
        "language/root*.langmeld",
        "language/CON.langmeld",
        "language/COM¹.langmeld",
        "language/root.langmeld.",
        "language/root.langmeld ",
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

    let error = ProjectManifest::parse("format = 1\ndefinition = \"language/CON.langmeld\"\n")
        .expect_err("reserved Windows device names are not portable");
    let message = error.to_string();

    assert!(message.contains("legacy `.lm` paths are accepted"));
    assert!(message.contains("Windows-invalid filename components"));
}

#[test]
fn reads_the_conventional_manifest_from_a_project_root() {
    let project = TestDirectory::new("manifest");
    project.write(PROJECT_MANIFEST_FILE, "format = 1\n");

    let manifest = read_project_manifest(project.path()).unwrap();

    assert_eq!(manifest.definition, DEFAULT_DEFINITION_PATH);
}

#[test]
fn accepts_legacy_lm_root_paths_for_migration() {
    let manifest = ProjectManifest::parse("format = 1\ndefinition = \"language/main.lm\"\n")
        .expect("legacy `.lm` root paths should remain loadable during migration");

    assert_eq!(manifest.definition, "language/main.lm");
}
