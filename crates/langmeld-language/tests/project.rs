use langmeld_language::{ProjectManifest, load_project_manifest, load_project_sources};
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
fn manifest_round_trips_and_rejects_unknown_fields_and_unsafe_paths() {
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
    assert!(
        manifest
            .to_json()
            .expect_err("paths cannot escape the project")
            .contains("without '..'")
    );
}

#[test]
fn source_loading_inherits_extensions_and_orders_discovered_paths() {
    let project = TempProject::new();
    project.write("examples/z.calc", "z source");
    project.write("examples/nested/a.calc", "a source");
    project.write("examples/ignored.txt", "ignored");
    let manifest = project.manifest(
        r#"{
  "schema_version": 1,
  "language": "language/main.lm",
  "sources": ["examples/z.calc"],
  "source_roots": ["examples"]
}
"#,
    );

    let sources = load_project_sources(&manifest, &[".calc".to_owned()])
        .expect("source roots use extensions from the language");
    assert_eq!(sources.len(), 2);
    assert!(sources[0].name.ends_with("examples/z.calc"));
    assert_eq!(sources[0].text, "z source");
    assert!(sources[1].name.ends_with("examples/nested/a.calc"));
    assert_eq!(sources[1].text, "a source");
}

#[test]
fn source_loading_rejects_empty_selection_missing_roots_and_invalid_files() {
    let project = TempProject::new();
    let empty = project.manifest(r#"{"schema_version":1,"language":"language/main.lm"}"#);
    assert!(
        load_project_sources(&empty, &[])
            .expect_err("empty source selection is invalid")
            .contains("must list")
    );

    let root_without_suffix = project.manifest(
        r#"{"schema_version":1,"language":"language/main.lm","source_roots":["examples"]}"#,
    );
    fs::create_dir(project.0.join("examples")).expect("source root is created");
    assert!(
        load_project_sources(&root_without_suffix, &[])
            .expect_err("source roots need suffixes")
            .contains("require source extensions")
    );

    project.write_bytes("bad.calc", &[0xff, 0xfe]);
    let invalid_utf8 = project
        .manifest(r#"{"schema_version":1,"language":"language/main.lm","sources":["bad.calc"]}"#);
    assert!(
        load_project_sources(&invalid_utf8, &[])
            .expect_err("source files must be UTF-8")
            .contains("not UTF-8")
    );
}
