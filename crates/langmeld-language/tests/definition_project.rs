use langmeld_language::{
    DefinitionLimits, langmeld_definition_language, parse_language_definition_project,
    parse_language_definition_project_with_limits, parse_language_definition_project_with_meta,
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
            "langmeld-definition-project-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temporary project directory is created");
        Self(path)
    }

    fn write(&self, relative: impl AsRef<Path>, source: &str) -> PathBuf {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("source parent directory is created");
        }
        fs::write(&path, source).expect("definition source is written");
        path
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const COMMON: &str = r#"language Catalog {
  version: "0.1.0"
  root: catalog

  type Metadata {
    label: string required
  }
}
"#;

const ENTRIES: &str = r#"language Catalog {
  version: "0.1.0"
  root: catalog
  import: "shared.lm"

  category Item {}

  construct item {
    syntax {
      default: "item {name}"
    }
    implements: Item
    fields {
      name: identifier required
      metadata: Metadata optional
    }
  }
}
"#;

const ROOT: &str = r#"language Catalog {
  version: "0.1.0"
  root: catalog
  import: "entries.lm"
  import: "shared.lm"

  construct catalog {
    syntax {
      root: "catalog {name} {"
    }
    fields {
      name: identifier required
    }
    block: [Item]
  }
}
"#;

#[test]
fn project_loader_resolves_recursive_imports_and_deduplicates_shared_files() {
    let project = TempProject::new();
    project.write("shared.lm", COMMON);
    project.write("entries.lm", ENTRIES);
    let root = project.write("catalog.lm", ROOT);

    let built_in = parse_language_definition_project(&root)
        .expect("the built-in project loader resolves recursive imports");
    let meta = langmeld_definition_language().expect("the bootstrap grammar validates");
    let supplied = parse_language_definition_project_with_meta(&root, &meta)
        .expect("the caller-supplied grammar resolves the same project");

    assert_eq!(built_in.spec(), supplied.spec());
    assert!(built_in.construct("Metadata").is_none());
    assert!(built_in.construct("item").is_some());
    assert!(built_in.construct("catalog").is_some());
    assert_eq!(built_in.spec().types.len(), 1);
}

#[test]
fn project_loader_discovers_types_before_conventional_constructs() {
    let project = TempProject::new();
    project.write(
        "types/10_metadata.lm",
        r#"types {
  type Metadata {
    label: string required
  }
}
"#,
    );
    project.write(
        "constructs/20_items.lm",
        r#"constructs {
  construct item {
    syntax {
      default: "item {name}"
    }
    implements: Item
    fields {
      name: identifier required
      metadata: Metadata optional
    }
  }
}
"#,
    );
    let root = project.write(
        "language.lm",
        r#"language Catalog {
  version: "0.1.0"
  root: catalog

  category Item {}

  construct catalog {
    syntax {
      root: "catalog {name} {"
    }
    fields {
      name: identifier required
    }
    block: [Item]
  }
}
"#,
    );

    let loaded = parse_language_definition_project(&root)
        .expect("conventional types and constructs modules are discovered");
    assert!(loaded.construct("item").is_some());
    assert_eq!(loaded.spec().types[0].id, "Metadata");
}

#[test]
fn project_loader_rejects_cycles_paths_outside_root_and_file_limit_overflow() {
    let project = TempProject::new();
    let root = project.write(
        "cycle.lm",
        "language Cycle {\n  version: \"0.1.0\"\n  root: program\n  import: \"part.lm\"\n}\n",
    );
    project.write(
        "part.lm",
        "language Cycle {\n  version: \"0.1.0\"\n  root: program\n  import: \"cycle.lm\"\n}\n",
    );
    let diagnostics =
        parse_language_definition_project(&root).expect_err("recursive imports must reject cycles");
    assert!(diagnostics[0].message.contains("import cycle"));

    let outside_project = TempProject::new();
    let outside_root = outside_project.write(
        "language/root.lm",
        "language Escape {\n  version: \"0.1.0\"\n  root: program\n  import: \"../outside.lm\"\n}\n",
    );
    outside_project.write("outside.lm", "language Outside {}\n");
    let diagnostics = parse_language_definition_project(&outside_root)
        .expect_err("canonical imports cannot escape the root directory");
    assert!(diagnostics[0].message.contains("escapes"));

    let limited_project = TempProject::new();
    let limited_root = limited_project.write(
        "language.lm",
        "language Limited {\n  version: \"0.1.0\"\n  root: program\n  import: \"module.lm\"\n}\n",
    );
    limited_project.write(
        "module.lm",
        "language Limited {\n  version: \"0.1.0\"\n  root: program\n}\n",
    );
    let diagnostics = parse_language_definition_project_with_limits(
        &limited_root,
        DefinitionLimits {
            max_files: 1,
            ..DefinitionLimits::default()
        },
    )
    .expect_err("the configured module count is enforced");
    assert!(diagnostics[0].message.contains("limit of 1 files"));

    let diagnostics = parse_language_definition_project_with_limits(
        &limited_root,
        DefinitionLimits {
            max_source_bytes: 1,
            ..DefinitionLimits::default()
        },
    )
    .expect_err("file reading is bounded by the project byte limit");
    assert!(diagnostics[0].message.contains("limit of 1 bytes"));
}

#[test]
fn project_loader_rejects_non_files_and_caps_conversion_diagnostics() {
    let project = TempProject::new();
    let non_file = project.0.join("not-a-file.lm");
    fs::create_dir(&non_file).expect("a directory with an lm suffix is created");
    let diagnostics = parse_language_definition_project(&non_file)
        .expect_err("definition paths must point to regular files");
    assert!(diagnostics[0].message.contains("regular file"));

    let invalid = project.write(
        "invalid.lm",
        r#"language Invalid {
  version: "not-semver"
  root: missing
  category Repeated {}
  category Repeated {}
  construct item {
    syntax { default: "item" }
  }
  construct item {
    syntax { default: "item" }
  }
}
"#,
    );
    let diagnostics = parse_language_definition_project_with_limits(
        &invalid,
        DefinitionLimits {
            max_diagnostics: 1,
            ..DefinitionLimits::default()
        },
    )
    .expect_err("invalid canonical models return bounded diagnostics");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "LMDEF102");
}
