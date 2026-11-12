use super::*;
use std::process::Command;

#[test]
fn check_uses_the_project_manifest_and_reports_the_language_name() {
    let parent = TestDirectory::new();
    let project = parent.path().join("widget-language");
    fs::create_dir_all(project.join("language")).expect("language directory should be created");
    fs::write(
        project.join("langmeld.toml"),
        "format = 1\ndefinition = \"language/language.langmeld\"\n",
    )
    .expect("manifest should be written");
    fs::write(
        project.join("language/language.langmeld"),
        "language \"Widget Script\" { construct \"item\" { syntax { literal \"item\"; } } }",
    )
    .expect("definition should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "check",
            "--manifest-path",
            project
                .join("langmeld.toml")
                .to_str()
                .expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Widget Script"));

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["check"])
        .current_dir(&project)
        .output()
        .expect("CLI should start from the project directory");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Widget Script"));
}

#[test]
fn check_reports_definition_diagnostics_with_source_locations() {
    let parent = TestDirectory::new();
    let project = parent.path().join("invalid-language");
    fs::create_dir_all(project.join("language")).expect("language directory should be created");
    fs::write(
        project.join("langmeld.toml"),
        "format = 1\ndefinition = \"language/language.langmeld\"\n",
    )
    .expect("manifest should be written");
    fs::write(
        project.join("language/language.langmeld"),
        "not a language definition",
    )
    .expect("invalid definition should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "check",
            "--manifest-path",
            project
                .join("langmeld.toml")
                .to_str()
                .expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");

    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("language/language.langmeld:1:"));
    assert!(error.contains("LM-DEF-"));
}

#[test]
fn check_warns_for_legacy_lm_imports_and_keeps_the_project_valid() {
    let parent = TestDirectory::new();
    let project = parent.path();
    fs::create_dir_all(project.join("language/constructs"))
        .expect("construct directory should be created");
    fs::write(
        project.join("langmeld.toml"),
        "format = 1\ndefinition = \"language/main.langmeld\"\n",
    )
    .expect("manifest should be written");
    fs::write(
        project.join("language/main.langmeld"),
        r#"language "Legacy" {
    import "constructs/item.lm";
    construct "program" { syntax { literal "program"; } }
}"#,
    )
    .expect("root definition should be written");
    fs::write(
        project.join("language/constructs/item.lm"),
        r#"construct "item" { syntax { literal "item"; } }"#,
    )
    .expect("legacy definition should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "check",
            "--manifest-path",
            project
                .join("langmeld.toml")
                .to_str()
                .expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let warning = String::from_utf8_lossy(&output.stderr);
    assert!(warning.contains("language/main.langmeld:2:"));
    assert!(warning.contains("warning[LM-DEF-W001]"));
    assert!(warning.contains("rename the file to `.langmeld`"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Language `Legacy` is valid."));
}

#[test]
fn check_warns_for_a_legacy_lm_root_and_keeps_the_project_valid() {
    let parent = TestDirectory::new();
    let project = parent.path();
    fs::create_dir_all(project.join("language")).expect("language directory should be created");
    fs::write(
        project.join("langmeld.toml"),
        "format = 1\ndefinition = \"language/main.lm\"\n",
    )
    .expect("manifest should be written");
    fs::write(
        project.join("language/main.lm"),
        r#"language "Legacy" { construct "program" { syntax { literal "program"; } } }"#,
    )
    .expect("legacy root definition should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["check", "--manifest-path", "langmeld.toml"])
        .current_dir(project)
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let warning = String::from_utf8_lossy(&output.stderr);
    assert!(warning.contains("language/main.lm:1:"));
    assert!(warning.contains("warning[LM-DEF-W001]"));
    assert!(warning.contains("update `definition`"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Language `Legacy` is valid."));
}

#[test]
fn check_explains_typed_binding_requirements_for_unnamed_repeated_groups() {
    let parent = TestDirectory::new();
    let project = parent.path();
    fs::create_dir_all(project.join("language")).expect("language directory should be created");
    fs::write(
        project.join("langmeld.toml"),
        "format = 1\nrust_bindings = \"typed\"\n",
    )
    .expect("manifest should be written");
    fs::write(
        project.join("language/main.langmeld"),
        r#"language "Widget" {
    construct "function" {
        syntax {
            literal "fn";
            one_or_more {
                identifier "parameter";
            }
        }
    }
}
"#,
    )
    .expect("definition should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .arg("check")
        .current_dir(project)
        .output()
        .expect("CLI should start");

    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("one_or_more \"name\""), "{error}");
    assert!(error.contains("ParsedConstruct::capture_groups"), "{error}");
}

#[test]
fn format_command_rewrites_valid_definitions_and_check_mode_is_read_only() {
    let parent = TestDirectory::new();
    let definition = parent.path().join("language.langmeld");
    let original = "language \"Toy\"{construct \"item\"{syntax{literal \"item\";}}}";
    fs::write(&definition, original).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["format", "--check"])
        .arg(&definition)
        .output()
        .expect("CLI should start");
    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(&definition).unwrap(), original);

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["format"])
        .arg(&definition)
        .output()
        .expect("CLI should start");
    assert!(output.status.success());
    let formatted = fs::read_to_string(&definition).unwrap();
    assert_eq!(
        formatted,
        "language \"Toy\" {\n    construct \"item\" {\n        syntax {\n            literal \"item\";\n        }\n    }\n}\n"
    );

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["format", "--check"])
        .arg(&definition)
        .output()
        .expect("CLI should start");
    assert!(output.status.success());

    let invalid_definition = parent.path().join("incomplete.langmeld");
    let invalid_source = "language \"Toy\" {";
    fs::write(&invalid_definition, invalid_source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["format"])
        .arg(&invalid_definition)
        .output()
        .expect("CLI should start");
    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(&invalid_definition).unwrap(),
        invalid_source
    );
}

#[test]
fn format_command_checks_and_formats_every_imported_project_definition() {
    let parent = TestDirectory::new();
    let project = parent.path();
    write_widget_project(project);
    let root = project.join("language/language.langmeld");
    let module = project.join("language/constructs/statements.langmeld");
    let original_root = fs::read_to_string(&root).unwrap();
    let original_module = fs::read_to_string(&module).unwrap();
    let manifest = project.join("langmeld.toml");

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["format", "--check"])
        .current_dir(project)
        .output()
        .expect("CLI should start");
    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(&root).unwrap(), original_root);
    assert_eq!(fs::read_to_string(&module).unwrap(), original_module);

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["format", "--manifest-path"])
        .arg(&manifest)
        .output()
        .expect("CLI should start");
    assert!(output.status.success());
    assert!(
        fs::read_to_string(&root)
            .unwrap()
            .contains("\n    construct \"program\" {")
    );
    assert!(
        fs::read_to_string(&module)
            .unwrap()
            .starts_with("construct \"print\" {\n")
    );

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["format", "--check", "--manifest-path"])
        .arg(&manifest)
        .output()
        .expect("CLI should start");
    assert!(output.status.success());
}

#[test]
fn format_command_does_not_write_a_project_with_invalid_imports() {
    let parent = TestDirectory::new();
    let project = parent.path();
    write_widget_project(project);
    let root = project.join("language/language.langmeld");
    let module = project.join("language/constructs/statements.langmeld");
    let original_root = fs::read_to_string(&root).unwrap();
    let invalid_module = "construct \"print\" { syntax { literal \"print\";";
    fs::write(&module, invalid_module).unwrap();
    let manifest = project.join("langmeld.toml");

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["format", "--manifest-path"])
        .arg(&manifest)
        .output()
        .expect("CLI should start");

    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(&root).unwrap(), original_root);
    assert_eq!(fs::read_to_string(&module).unwrap(), invalid_module);
}

#[cfg(unix)]
#[test]
fn format_command_refuses_to_replace_a_symlink_target() {
    use std::os::unix::fs::symlink;

    let parent = TestDirectory::new();
    let target = parent.path().join("target.langmeld");
    let link = parent.path().join("linked.langmeld");
    let original = "language \"Toy\"{construct \"item\"{syntax{literal \"item\";}}}";
    fs::write(&target, original).unwrap();
    symlink(&target, &link).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["format"])
        .arg(&link)
        .output()
        .expect("CLI should start");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("refusing to replace symlink"));
    assert_eq!(fs::read_to_string(&target).unwrap(), original);
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}
