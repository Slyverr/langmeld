use langmeld_language::{
    DefinitionLimits, langmeld_definition_language, parse_language_definition_project,
    parse_language_definition_project_with_limits, parse_language_definition_project_with_meta,
    parse_language_definition_project_with_overlays,
    parse_language_definition_project_with_overlays_and_diagnostics,
    parse_language_definition_project_with_overlays_and_limits,
};
use std::{
    collections::BTreeMap,
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
  import: "shared.langmeld"

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
  import: "entries.langmeld"
  import: "shared.langmeld"

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
    project.write("shared.langmeld", COMMON);
    project.write("entries.langmeld", ENTRIES);
    let root = project.write("catalog.langmeld", ROOT);

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
fn file_loader_accepts_langmeld_and_rejects_legacy_lm_definition_suffix() {
    let project = TempProject::new();
    project.write("shared.langmeld", COMMON);
    project.write("entries.langmeld", ENTRIES);
    let root = project.write("catalog.langmeld", ROOT);

    parse_language_definition_project(&root).expect("the canonical definition suffix is accepted");

    let legacy = project.write("legacy.lm", ROOT);
    let diagnostics = parse_language_definition_project(&legacy)
        .expect_err("the old definition suffix is rejected after migration");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.message.contains("legacy `.lm`") && diagnostic.message.contains("`.langmeld`")
    }));

    let legacy_import = project.write(
        "legacy-import.langmeld",
        &ROOT.replace("entries.langmeld", "entries.lm"),
    );
    let diagnostics = parse_language_definition_project(&legacy_import)
        .expect_err("legacy import suffixes are rejected before file discovery");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.message.contains("legacy `.lm`") && diagnostic.message.contains("`.langmeld`")
    }));
}

#[test]
fn project_loader_uses_unsaved_overlays_without_changing_disk_sources() {
    let project = TempProject::new();
    project.write("shared.langmeld", COMMON);
    let entries = project.write("entries.langmeld", ENTRIES);
    let root = project.write("catalog.langmeld", ROOT);
    let changed_root = BTreeMap::from([(root.clone(), ROOT.replace("0.1.0", "0.2.0"))]);
    let changed_import = BTreeMap::from([(entries.clone(), ENTRIES.replace("0.1.0", "0.2.0"))]);

    let diagnostics = parse_language_definition_project_with_overlays(&root, &changed_root)
        .expect_err("the unsaved root version must be checked against imported modules");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("different language name, version, or root")
    }));

    let diagnostics = parse_language_definition_project_with_overlays(&root, &changed_import)
        .expect_err("an unsaved imported-module version must be checked against the root");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("different language name, version, or root")
    }));
    assert_eq!(
        fs::read_to_string(&root).expect("root remains on disk"),
        ROOT
    );
    assert_eq!(
        fs::read_to_string(&entries).expect("import remains on disk"),
        ENTRIES
    );
}

#[test]
fn project_loader_rejects_overlays_outside_the_root_and_bounds_overlay_text() {
    let project = TempProject::new();
    let root = project.write("language.langmeld", ROOT);
    let outside_project = TempProject::new();
    let outside = outside_project.write("outside.langmeld", ROOT);
    let outside_overlays = BTreeMap::from([(outside.clone(), ROOT.to_owned())]);

    let diagnostics = parse_language_definition_project_with_overlays(&root, &outside_overlays)
        .expect_err("an overlay cannot cross the language root boundary");
    assert!(
        diagnostics[0]
            .message
            .contains("escapes the language directory")
    );

    let oversized_overlays = BTreeMap::from([(root.clone(), ROOT.to_owned())]);
    let diagnostics = parse_language_definition_project_with_overlays_and_limits(
        &root,
        &oversized_overlays,
        DefinitionLimits {
            max_source_bytes: 1,
            ..DefinitionLimits::default()
        },
    )
    .expect_err("overlay text is bounded before parsing");
    assert!(
        diagnostics[0]
            .message
            .contains("overlays exceed the total parsing limit")
    );
}

#[test]
fn project_parser_retains_imported_file_identity_for_syntax_diagnostics() {
    let project = TempProject::new();
    let entries = project.write("entries.langmeld", ENTRIES);
    let root = project.write("catalog.langmeld", ROOT);
    let overlays = BTreeMap::from([(entries.clone(), "language Catalog {".to_owned())]);

    let diagnostics = parse_language_definition_project_with_overlays_and_diagnostics(
        &root,
        &overlays,
        DefinitionLimits::default(),
    )
    .expect_err("the imported module has incomplete syntax");

    let entry_path = fs::canonicalize(&entries).expect("import path is canonical");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].source_path.as_ref(), Some(&entry_path));
    assert!(diagnostics[0].diagnostic.span.is_some());
    assert_eq!(
        diagnostics[0]
            .source
            .as_deref()
            .map(|source| source.text.as_str()),
        Some("language Catalog {")
    );
}

#[test]
fn project_parser_does_not_guess_a_file_for_cross_module_diagnostics() {
    let project = TempProject::new();
    project.write("shared.langmeld", COMMON);
    project.write("entries.langmeld", ENTRIES);
    let root = project.write("catalog.langmeld", ROOT);
    let overlays = BTreeMap::from([(root.clone(), ROOT.replace("0.1.0", "0.2.0"))]);

    let diagnostics = parse_language_definition_project_with_overlays_and_diagnostics(
        &root,
        &overlays,
        DefinitionLimits::default(),
    )
    .expect_err("the root and imported module target different versions");

    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic
                .diagnostic
                .message
                .contains("different language name, version, or root")
        })
        .unwrap_or_else(|| panic!("cross-module incompatibility is diagnosed: {diagnostics:#?}"));
    assert_eq!(diagnostic.source_path, None);
}

#[test]
fn project_loader_discovers_types_before_conventional_constructs() {
    let project = TempProject::new();
    project.write(
        "types/10_metadata.langmeld",
        r#"types {
  type Metadata {
    label: string required
  }
}
"#,
    );
    project.write(
        "constructs/20_items.langmeld",
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
        "language.langmeld",
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
        "cycle.langmeld",
        "language Cycle {\n  version: \"0.1.0\"\n  root: program\n  import: \"part.langmeld\"\n}\n",
    );
    project.write(
        "part.langmeld",
        "language Cycle {\n  version: \"0.1.0\"\n  root: program\n  import: \"cycle.langmeld\"\n}\n",
    );
    let diagnostics =
        parse_language_definition_project(&root).expect_err("recursive imports must reject cycles");
    assert!(diagnostics[0].message.contains("import cycle"));

    let outside_project = TempProject::new();
    let outside_root = outside_project.write(
        "language/root.langmeld",
        "language Escape {\n  version: \"0.1.0\"\n  root: program\n  import: \"../outside.langmeld\"\n}\n",
    );
    outside_project.write("outside.langmeld", "language Outside {}\n");
    let diagnostics = parse_language_definition_project(&outside_root)
        .expect_err("canonical imports cannot escape the root directory");
    assert!(diagnostics[0].message.contains("escapes"));

    let limited_project = TempProject::new();
    let limited_root = limited_project.write(
        "language.langmeld",
        "language Limited {\n  version: \"0.1.0\"\n  root: program\n  import: \"module.langmeld\"\n}\n",
    );
    limited_project.write(
        "module.langmeld",
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
    let non_file = project.0.join("not-a-file.langmeld");
    fs::create_dir(&non_file).expect("a directory with an lm suffix is created");
    let diagnostics = parse_language_definition_project(&non_file)
        .expect_err("definition paths must point to regular files");
    assert!(diagnostics[0].message.contains("regular file"));

    let invalid = project.write(
        "invalid.langmeld",
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
