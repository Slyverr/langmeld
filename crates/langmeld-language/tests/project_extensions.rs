use langmeld_language::{
    ExtensionDependency, ExtensionPackage, ManifestExtension, ProjectManifest,
    load_project_manifest, load_project_with_manifest,
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
        let path = std::env::temp_dir().join(format!(
            "langmeld-project-extensions-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temporary project directory is created");
        Self(path)
    }

    fn write(&self, relative: impl AsRef<Path>, contents: &str) {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent directory is created");
        }
        fs::write(path, contents).expect("project file is written");
    }

    fn manifest(&self, extensions: Vec<ManifestExtension>) -> ProjectManifest {
        let mut manifest = ProjectManifest::new("language.lm");
        manifest.sources.push(PathBuf::from("source.calc"));
        manifest.extensions = extensions;
        manifest
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

#[test]
fn project_composes_extensions_in_stable_dependency_order() {
    let project = TempProject::new();
    project.write("language.lm", BASE);
    project.write(
        "extensions/first.lm",
        r#"language Catalog {
  version: "0.1.0"
  root: catalog
  construct first {
    syntax { default: "first {name}" }
    implements: Item
    fields { name: identifier required }
  }
}
"#,
    );
    project.write(
        "extensions/second.lm",
        r#"language Catalog {
  version: "0.1.0"
  root: catalog
  construct second {
    syntax { default: "second {name}" }
    implements: Item
    fields { name: identifier required }
  }
}
"#,
    );
    project.write("source.calc", "catalog demo { item first }\n");

    let manifest = project.manifest(vec![
        ManifestExtension::Package(ExtensionPackage {
            id: "second".to_owned(),
            version: "1.0.0".to_owned(),
            path: PathBuf::from("extensions/second.lm"),
            language_version: "^0.1".to_owned(),
            dependencies: vec![ExtensionDependency {
                id: "first".to_owned(),
                version: "^2.0".to_owned(),
            }],
        }),
        ManifestExtension::Package(ExtensionPackage {
            id: "first".to_owned(),
            version: "2.1.0".to_owned(),
            path: PathBuf::from("extensions/first.lm"),
            language_version: "^0.1".to_owned(),
            dependencies: Vec::new(),
        }),
    ]);
    let manifest_path = project.0.join("langmeld.json");
    project.write(
        "langmeld.json",
        &manifest.to_json().expect("extension manifest serializes"),
    );
    let manifest = load_project_manifest(&manifest_path).expect("extension manifest loads");

    let loaded = load_project_with_manifest(&manifest_path, &manifest)
        .expect("compatible local extensions compose");
    let construct_ids: Vec<_> = loaded
        .language
        .spec()
        .constructs
        .iter()
        .map(|construct| construct.id.as_str())
        .collect();
    assert_eq!(construct_ids, ["catalog", "item", "first", "second"]);
}

#[test]
fn project_rejects_extension_cycles_and_unsatisfied_dependencies() {
    let project = TempProject::new();
    project.write("language.lm", BASE);
    project.write("source.calc", "catalog demo {}\n");
    let package = |id: &str, dependency: &str| {
        ManifestExtension::Package(ExtensionPackage {
            id: id.to_owned(),
            version: "1.0.0".to_owned(),
            path: PathBuf::from(format!("extensions/{id}.lm")),
            language_version: "^0.1".to_owned(),
            dependencies: vec![ExtensionDependency {
                id: dependency.to_owned(),
                version: "^1.0".to_owned(),
            }],
        })
    };

    let cycle = project.manifest(vec![package("alpha", "beta"), package("beta", "alpha")]);
    let error = load_project_with_manifest(&project.0.join("langmeld.json"), &cycle)
        .expect_err("extension cycles are rejected");
    assert!(error.contains("dependency cycle"));

    let missing = project.manifest(vec![package("alpha", "missing")]);
    let error = load_project_with_manifest(&project.0.join("langmeld.json"), &missing)
        .expect_err("undeclared extension dependencies are rejected");
    assert!(error.contains("not declared"));
}

#[test]
fn project_rejects_extension_language_version_mismatch_before_loading_definition() {
    let project = TempProject::new();
    project.write("language.lm", BASE);
    project.write("source.calc", "catalog demo {}\n");
    let manifest = project.manifest(vec![ManifestExtension::Package(ExtensionPackage {
        id: "compatibility".to_owned(),
        version: "1.0.0".to_owned(),
        path: PathBuf::from("missing.lm"),
        language_version: "^2.0".to_owned(),
        dependencies: Vec::new(),
    })]);

    let error = load_project_with_manifest(&project.0.join("langmeld.json"), &manifest)
        .expect_err("incompatible package requirements fail");
    assert!(error.contains("requires language"));
}

#[test]
fn project_accepts_an_unversioned_local_definition_path() {
    let project = TempProject::new();
    project.write("language.lm", BASE);
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
    let manifest = project.manifest(vec![ManifestExtension::Path(PathBuf::from(
        "extensions/local.lm",
    ))]);

    let loaded = load_project_with_manifest(&project.0.join("langmeld.json"), &manifest)
        .expect("local extension paths compose");
    assert!(loaded.language.construct("local").is_some());
}

#[test]
fn project_rejects_extension_lists_above_the_resource_limit() {
    let mut manifest = ProjectManifest::new("language.lm");
    manifest.extensions = (0..129)
        .map(|index| ManifestExtension::Path(PathBuf::from(format!("extensions/{index}.lm"))))
        .collect();

    let error = manifest
        .to_json()
        .expect_err("oversized extension lists are rejected before loading files");
    assert!(error.contains("project extension count exceeds 128"));
}
