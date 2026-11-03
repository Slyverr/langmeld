use langmeld_language::{
    ProjectManifest, load_language, load_project, load_project_manifest, load_project_sources,
    load_project_with_manifest, load_source_files, write_project_manifest,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("langmeld-project-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).expect("temporary project directory is created");
        Self(path)
    }

    fn write(&self, relative: impl AsRef<Path>, contents: &str) -> PathBuf {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent directory is created");
        }
        fs::write(&path, contents).expect("project file is written");
        path
    }

    fn manifest(&self, contents: &str) -> PathBuf {
        self.write("langmeld.json", contents)
    }

    fn write_language(&self) {
        self.write(
            "language/language.lm",
            r#"language Sample {
  version: "0.1.0"
  root: program
  source_extension: ".calc"
  import: "constructs/program.lm"
}
"#,
        );
        self.write(
            "language/constructs/program.lm",
            r#"constructs {
  construct program {
    syntax { default: "program {" }
    block: [statement]
  }
  construct statement {
    syntax { default: "print {message}" }
    fields { message: text required }
  }
}
"#,
        );
    }

    fn write_bytes(&self, relative: impl AsRef<Path>, contents: &[u8]) -> PathBuf {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent directory is created");
        }
        fs::write(&path, contents).expect("project file is written");
        path
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn manifest_round_trips_rejects_unknown_fields_and_allows_workspace_paths() {
    let mut manifest = ProjectManifest::new("language/main.lm");
    manifest.source_roots.push(PathBuf::from("examples"));
    let json = manifest.to_json().expect("manifest serializes");
    let project = TempProject::new();
    let path = project.manifest(&json);
    assert_eq!(
        load_project_manifest(&path).expect("manifest loads"),
        manifest
    );

    let unknown =
        project.manifest(r#"{"schema_version":1,"language":"language/main.lm","surprise":true}"#);
    assert!(
        load_project_manifest(&unknown)
            .expect_err("unknown manifest fields are rejected")
            .contains("unknown field")
    );

    manifest.language = PathBuf::from("../outside.lm");
    manifest.language = PathBuf::from("../shared/language.lm");
    assert!(manifest.to_json().is_ok());
}

#[test]
fn source_loading_inherits_extensions_and_orders_discovered_paths() {
    let project = TempProject::new();
    project.write_language();
    project.write("examples/z.calc", "z source");
    project.write("examples/nested/a.calc", "a source");
    project.write("examples/ignored.txt", "ignored");
    let manifest = project.manifest(
        r#"{
  "schema_version": 1,
  "language": "language/language.lm",
  "sources": ["examples/z.calc"],
  "source_roots": ["examples"]
}
"#,
    );

    let project_manifest = load_project_manifest(&manifest).expect("manifest is valid");
    let sources = load_project_sources(&manifest, &project_manifest)
        .expect("source roots inherit extensions from the language definition");
    assert_eq!(sources.len(), 2);
    assert!(sources[0].name.ends_with("examples/z.calc"));
    assert_eq!(sources[0].text, "z source");
    assert!(sources[1].name.ends_with("examples/nested/a.calc"));
    assert_eq!(sources[1].text, "a source");

    let snapshot = load_project(&manifest).expect("project snapshot resolves definition and files");
    assert_eq!(snapshot.language.spec().source_extensions, [".calc"]);
    assert_eq!(snapshot.sources.len(), 2);
    assert_eq!(
        snapshot.manifest.language,
        PathBuf::from("language/language.lm")
    );
}

#[test]
fn candidate_project_validation_and_manifest_write_are_separate() {
    let project = TempProject::new();
    project.write_language();
    project.write("examples/hello.calc", "print \"hello\"\n");
    let manifest_path = project.manifest(
        r#"{"schema_version":1,"language":"language/language.lm","source_roots":["examples"]}"#,
    );
    let original = fs::read(&manifest_path).expect("original manifest bytes are readable");
    let mut candidate = load_project_manifest(&manifest_path).expect("manifest loads");
    candidate.source_roots = vec![PathBuf::from("missing")];

    assert!(
        load_project_with_manifest(&manifest_path, &candidate)
            .expect_err("invalid candidate source roots fail validation")
            .contains("cannot inspect source root")
    );
    assert_eq!(
        fs::read(&manifest_path).expect("manifest remains readable"),
        original
    );

    candidate.source_roots = vec![PathBuf::from("examples")];
    candidate.sources = vec![PathBuf::from("examples/hello.calc")];
    write_project_manifest(&manifest_path, &candidate).expect("valid manifest replaces atomically");
    let written = load_project_manifest(&manifest_path).expect("updated manifest loads");
    assert_eq!(written, candidate);
    let loaded = load_project(&manifest_path).expect("updated project resolves");
    assert_eq!(
        loaded.sources.len(),
        1,
        "explicit and discovered paths deduplicate"
    );
}

#[cfg(unix)]
#[test]
fn project_manifest_loader_and_writer_refuse_symlinks() {
    use std::os::unix::fs::symlink;

    let project = TempProject::new();
    let target = project.manifest(
        r#"{"schema_version":1,"language":"language/language.lm","sources":["source.calc"]}"#,
    );
    let link = project.0.join("linked.json");
    symlink(&target, &link).expect("manifest symlink is created");
    assert!(
        load_project_manifest(&link)
            .expect_err("manifest symlinks are rejected")
            .contains("not a regular file")
    );
    let manifest = ProjectManifest::new("language/language.lm");
    assert!(
        write_project_manifest(&link, &manifest)
            .expect_err("manifest writer refuses symlinks")
            .contains("not a regular file")
    );
}

#[test]
fn standalone_json_language_and_direct_sources_share_project_input_limits() {
    use langmeld_core::{ConstructBuilder, LanguageBuilder};

    let project = TempProject::new();
    let language = LanguageBuilder::new("JsonLanguage")
        .version("0.1.0")
        .root("program")
        .source_extension(".calc")
        .construct(
            ConstructBuilder::new("program")
                .syntax("program {")
                .block(["entry"]),
        )
        .construct(ConstructBuilder::new("entry").syntax("entry"))
        .build()
        .expect("JSON language model is valid");
    let json = serde_json::to_string(language.spec()).expect("language serializes to JSON");
    let definition = project.write("language.json", &json);
    let loaded = load_language(&definition).expect("standalone JSON language loads");
    assert_eq!(loaded.spec().source_extensions, [".calc"]);

    let source = project.write("program.calc", "program {}\n");
    let loaded_sources = load_source_files(&[source.clone(), source])
        .expect("direct source inputs are bounded and de-duplicated");
    assert_eq!(loaded_sources.len(), 1);
    assert_eq!(loaded_sources[0].text, "program {}\n");
}

#[test]
fn source_loading_rejects_empty_selection_missing_roots_and_invalid_files() {
    let project = TempProject::new();
    project.write_language();
    let empty = project.manifest(r#"{"schema_version":1,"language":"language/language.lm"}"#);
    let empty_manifest = load_project_manifest(&empty).expect("manifest is valid");
    assert!(
        load_project_sources(&empty, &empty_manifest)
            .expect_err("empty source selection is invalid")
            .contains("must list")
    );

    let empty_root = project.manifest(
        r#"{"schema_version":1,"language":"language/language.lm","source_roots":["examples"]}"#,
    );
    fs::create_dir(project.0.join("examples")).expect("source root is created");
    let empty_root_manifest = load_project_manifest(&empty_root).expect("manifest is valid");
    assert!(
        load_project_sources(&empty_root, &empty_root_manifest)
            .expect_err("empty source roots are invalid")
            .contains("resolved no source files")
    );

    project.write_bytes("bad.calc", &[0xff, 0xfe]);
    let invalid_utf8 = project.manifest(
        r#"{"schema_version":1,"language":"language/language.lm","sources":["bad.calc"]}"#,
    );
    let invalid_manifest = load_project_manifest(&invalid_utf8).expect("manifest is valid");
    assert!(
        load_project_sources(&invalid_utf8, &invalid_manifest)
            .expect_err("source files must be UTF-8")
            .contains("not UTF-8")
    );
}
