use langmeld_core::{ConstructBuilder, LanguageBuilder};
use langmeld_language::{
    ExtensionPackage, ManifestExtension, ProjectBuildConfig, ProjectManifest, load_project,
    load_project_manifest, write_project_lockfile, write_project_manifest,
};
use serde_json::Value;
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
            std::env::temp_dir().join(format!("langmeld-project-lock-{}-{id}", std::process::id()));
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

    fn write_project(&self, manifest: &ProjectManifest) -> PathBuf {
        let path = self.0.join("langmeld.json");
        write_project_manifest(&path, manifest).expect("project manifest is written");
        path
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const BASE: &str = r#"language Catalog {
  version: "0.1.0"
  root: catalog
  source_extension: ".calc"
  category Item {}
  construct catalog {
    syntax { root: "catalog {name} {" }
    fields { name: identifier required }
    block: [Item]
  }
  construct item {
    syntax { default: "item {name}" }
    implements: Item
    fields { name: identifier required }
  }
}
"#;

fn starter_manifest() -> ProjectManifest {
    let mut manifest = ProjectManifest::new("language.lm");
    manifest.sources.push(PathBuf::from("source.calc"));
    manifest
}

#[test]
fn lockfile_is_deterministic_and_ignores_non_language_manifest_settings() {
    let project = TempProject::new();
    project.write("language.lm", BASE);
    project.write("source.calc", "catalog demo { item entry }\n");
    project.write("source-root/other.calc", "catalog other {}\n");
    let manifest_path = project.write_project(&starter_manifest());

    assert!(
        load_project(&manifest_path).is_ok(),
        "missing locks are allowed"
    );
    let lock_path = write_project_lockfile(&manifest_path).expect("lockfile writes");
    let first = fs::read(&lock_path).expect("lock bytes are readable");
    write_project_lockfile(&manifest_path).expect("lockfile refreshes");
    assert_eq!(
        fs::read(&lock_path).expect("lock bytes are readable"),
        first
    );

    let lock: Value = serde_json::from_slice(&first).expect("lockfile is JSON");
    assert_eq!(lock["schema_version"], 1);
    assert_eq!(lock["definitions"][0]["path"], "language.lm");
    assert_eq!(lock["definitions"][0]["sha256"].as_str().unwrap().len(), 64);

    let mut manifest = load_project_manifest(&manifest_path).expect("manifest loads");
    manifest.source_extensions = vec![".calc".to_owned()];
    manifest.source_roots.push(PathBuf::from("source-root"));
    manifest.build = Some(ProjectBuildConfig::new("test-backend"));
    write_project_manifest(&manifest_path, &manifest).expect("source and build settings update");
    load_project(&manifest_path).expect("non-language settings do not stale the lock");
}

#[test]
fn lockfile_pins_imported_definition_bytes_and_rejects_stale_content() {
    let project = TempProject::new();
    project.write(
        "types/shared.lm",
        "types {\n  enum State {\n    ready\n    draft\n  }\n}\n",
    );
    project.write(
        "language.lm",
        r#"language Catalog {
  version: "0.1.0"
  root: catalog
  source_extension: ".calc"
  import: "types/shared.lm"
  category Item {}
  construct catalog {
    syntax { root: "catalog {name} {" }
    fields { name: identifier required }
    block: [Item]
  }
  construct item {
    syntax { default: "item {name}" }
    implements: Item
    fields { name: identifier required }
  }
}
"#,
    );
    project.write(
        "extensions/local.lm",
        r#"language Catalog {
  version: "0.1.0"
  root: catalog
  construct local {
    syntax { default: "local {name}" }
    implements: Item
    fields { name: identifier required }
  }
}
"#,
    );
    project.write("source.calc", "catalog demo {}\n");
    let mut manifest = starter_manifest();
    manifest.extensions = vec![ManifestExtension::Path(PathBuf::from(
        "extensions/local.lm",
    ))];
    let manifest_path = project.write_project(&manifest);

    let lock_path = write_project_lockfile(&manifest_path).expect("lockfile writes");
    let lock: Value = serde_json::from_slice(&fs::read(&lock_path).expect("lock reads"))
        .expect("lockfile is JSON");
    let paths: Vec<_> = lock["definitions"]
        .as_array()
        .expect("definition list is an array")
        .iter()
        .map(|definition| definition["path"].as_str().expect("path is a string"))
        .collect();
    assert_eq!(
        paths,
        ["types/shared.lm", "language.lm", "extensions/local.lm"]
    );
    assert_eq!(
        lock["resolution_sha256"],
        "b1157681325ca24adc949edaabe2c591b0cbd7233cb464216ef9ed6f7d3bcb01"
    );
    load_project(&manifest_path).expect("matching lockfile loads");

    project.write(
        "types/shared.lm",
        "types {\n  enum State {\n    ready\n    draft\n  }\n}\n# changed\n",
    );
    let error = load_project(&manifest_path).expect_err("changed imports stale the lock");
    assert!(error.contains("lockfile") && error.contains("stale"));

    write_project_lockfile(&manifest_path).expect("refreshed lock writes");
    let mut base =
        fs::read_to_string(project.0.join("language.lm")).expect("base language definition reads");
    base.push_str("\n# base definition changed\n");
    project.write("language.lm", &base);
    let error = load_project(&manifest_path).expect_err("changed base bytes stale the lock");
    assert!(error.contains("lockfile") && error.contains("stale"));
}

#[test]
fn lockfile_accepts_crlf_checkout_of_locked_definition_sources() {
    let project = TempProject::new();
    project.write("language.lm", BASE);
    project.write("source.calc", "catalog demo {}\n");
    let manifest_path = project.write_project(&starter_manifest());
    let lock_path = write_project_lockfile(&manifest_path).expect("lockfile writes");

    let language_path = project.0.join("language.lm");
    let language = fs::read_to_string(&language_path).expect("language source reads");
    fs::write(&language_path, language.replace('\n', "\r\n"))
        .expect("language source is rewritten with CRLF endings");

    load_project(&manifest_path).expect("line-ending conversion does not stale the lock");
    let lock: Value = serde_json::from_slice(&fs::read(lock_path).expect("lock reads"))
        .expect("lockfile is JSON");
    assert_eq!(lock["definitions"][0]["path"], "language.lm");
}

#[test]
fn lockfile_tracks_package_metadata_and_definition_edits() {
    let project = TempProject::new();
    project.write("language.lm", BASE);
    let extension = r#"language Catalog {
  version: "0.1.0"
  root: catalog
  construct network {
    syntax { default: "network {name}" }
    implements: Item
    fields { name: identifier required }
  }
}
"#;
    project.write("extensions/network.lm", extension);
    project.write("source.calc", "catalog demo {}\n");
    let mut manifest = starter_manifest();
    manifest.extensions = vec![ManifestExtension::Package(ExtensionPackage {
        id: "network".to_owned(),
        version: "1.2.0".to_owned(),
        path: PathBuf::from("extensions/network.lm"),
        language_version: "^0.1".to_owned(),
        dependencies: Vec::new(),
    })];
    let manifest_path = project.write_project(&manifest);
    let lock_path = write_project_lockfile(&manifest_path).expect("lockfile writes");
    let lock: Value = serde_json::from_slice(&fs::read(lock_path).expect("lock reads"))
        .expect("lockfile is JSON");
    assert_eq!(
        lock["resolution_sha256"],
        "f416b31b9848ac3c5d0606615c1e69a51865ab2174f94a90c0832cd8acf563e0"
    );

    manifest.extensions[0] = ManifestExtension::Package(ExtensionPackage {
        version: "1.3.0".to_owned(),
        ..match &manifest.extensions[0] {
            ManifestExtension::Package(package) => package.clone(),
            ManifestExtension::Path(_) => unreachable!(),
        }
    });
    write_project_manifest(&manifest_path, &manifest).expect("package version updates");
    let error = load_project(&manifest_path).expect_err("package metadata change stales the lock");
    assert!(error.contains("stale"));

    write_project_lockfile(&manifest_path).expect("updated lockfile writes");
    project.write(
        "extensions/network.lm",
        &format!("{extension}\n# changed\n"),
    );
    let error = load_project(&manifest_path).expect_err("extension bytes stale the lock");
    assert!(error.contains("stale"));
}

#[test]
fn lockfile_loader_rejects_unsupported_malformed_and_symlink_locks() {
    let project = TempProject::new();
    project.write("language.lm", BASE);
    project.write("source.calc", "catalog demo {}\n");
    let manifest_path = project.write_project(&starter_manifest());
    let lock_path = project.0.join("langmeld.lock");

    fs::write(&lock_path, r#"{"schema_version":99}"#).expect("unsupported lock writes");
    assert!(
        load_project(&manifest_path)
            .unwrap_err()
            .contains("unsupported project lockfile schema")
    );
    fs::write(&lock_path, "not json").expect("malformed lock writes");
    assert!(
        load_project(&manifest_path)
            .unwrap_err()
            .contains("cannot parse project lockfile")
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let target = project.write("outside.lock", "target remains unchanged");
        fs::remove_file(&lock_path).expect("malformed lock is removed");
        symlink(&target, &lock_path).expect("lock symlink is created");
        assert!(
            load_project(&manifest_path)
                .unwrap_err()
                .contains("not a regular file")
        );
        assert!(
            write_project_lockfile(&manifest_path)
                .unwrap_err()
                .contains("not a regular file")
        );
        assert_eq!(
            fs::read_to_string(target).expect("target remains readable"),
            "target remains unchanged"
        );
    }
}

#[test]
fn lockfile_pins_standalone_json_language_definitions() {
    let project = TempProject::new();
    let language = LanguageBuilder::new("JsonCatalog")
        .version("0.1.0")
        .root("program")
        .source_extension(".calc")
        .construct(
            ConstructBuilder::new("program")
                .syntax("program {")
                .block(std::iter::empty::<&str>()),
        )
        .build()
        .expect("JSON language is valid");
    project.write(
        "language.json",
        &serde_json::to_string(language.spec()).expect("language serializes"),
    );
    project.write("source.calc", "program {\n}\n");
    let mut manifest = starter_manifest();
    manifest.language_source = langmeld_language::ProjectLanguageSource::Declarative {
        path: PathBuf::from("language.json"),
    };
    let manifest_path = project.write_project(&manifest);

    write_project_lockfile(&manifest_path).expect("JSON language lock writes");
    load_project(&manifest_path).expect("matching JSON lock loads");
    project.write(
        "language.json",
        &format!(
            "{}\n",
            serde_json::to_string(language.spec()).expect("language serializes")
        ),
    );
    let error = load_project(&manifest_path).expect_err("changed JSON bytes stale the lock");
    assert!(error.contains("stale"));
}

#[test]
fn version_two_declarative_manifest_keeps_definition_lock_behavior() {
    let project = TempProject::new();
    project.write("language.lm", BASE);
    project.write("source.calc", "catalog demo {}\n");
    let mut manifest = ProjectManifest::new_declarative("language.lm");
    manifest.sources.push(PathBuf::from("source.calc"));
    let manifest_path = project.write_project(&manifest);

    write_project_lockfile(&manifest_path).expect("v2 declarative lock writes");
    load_project(&manifest_path).expect("v2 declarative manifest and lock load");
}
