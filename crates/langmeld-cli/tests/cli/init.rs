use super::*;
use std::process::Command;

#[test]
fn init_generates_a_modular_declarative_project_without_overwriting_files() {
    let parent = TestDirectory::new();
    let project = parent.path().join("widget-language");
    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--name",
            "widget-language",
            "--language-name",
            r#"Widget "Script""#,
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("langmeld lsp`"));
    assert!(project.join("Cargo.toml").is_file());
    assert_eq!(
        fs::read_to_string(project.join("LICENSE-MIT")).unwrap(),
        include_str!("../../templates/LICENSE-MIT")
    );
    assert_eq!(
        fs::read_to_string(project.join("LICENSE-APACHE")).unwrap(),
        include_str!("../../templates/LICENSE-APACHE")
    );
    assert!(project.join("langmeld.toml").is_file());
    assert!(project.join("src/lib.rs").is_file());
    assert!(project.join("src/main.rs").is_file());
    assert!(project.join("src/runtime.rs").is_file());
    assert!(project.join("src/constructs/mod.rs").is_file());
    assert!(project.join("src/constructs/let_handler.rs").is_file());
    assert!(project.join("src/constructs/print.rs").is_file());
    assert!(project.join("tests/language.rs").is_file());
    assert!(project.join("language/main.langmeld").is_file());
    assert!(!project.join("AGENTS.md").exists());
    assert!(!project.join(".agents").exists());
    assert!(!project.join("language/language.langmeld").exists());
    assert!(
        project
            .join("language/constructs/statements.langmeld")
            .is_file()
    );
    let main = fs::read_to_string(project.join("src/main.rs")).expect("CLI should be generated");
    assert!(main.contains("args_os()"));
    assert!(main.contains("fs::read_to_string"));
    let library = fs::read_to_string(project.join("src/lib.rs"))
        .expect("reusable language loader should be generated");
    assert!(!library.contains('\r'));
    assert!(library.ends_with("}\n"));
    assert!(!library.ends_with("}\n\n"));
    let manifest =
        fs::read_to_string(project.join("Cargo.toml")).expect("manifest should be generated");
    assert!(manifest.contains("[workspace]"));
    assert!(manifest.contains("license = \"MIT OR Apache-2.0\""));
    let readme = fs::read_to_string(project.join("README.md")).expect("README should be generated");
    assert!(readme.contains("## License"));
    assert!(readme.contains("language/main.langmeld"));
    assert!(readme.contains("src/constructs/"));
    assert!(readme.contains("langmeld generate construct repeat"));
    assert!(readme.contains("generated handler reports a diagnostic"));
    let manifest = fs::read_to_string(project.join("langmeld.toml"))
        .expect("Langmeld manifest should be generated");
    assert_eq!(manifest, "format = 1\n");
    let check = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .current_dir(&project)
        .arg("check")
        .output()
        .expect("CLI should start");
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    let definition = fs::read_to_string(project.join("language/main.langmeld"))
        .expect("definition should be generated");
    assert!(definition.starts_with(r#"language "Widget \"Script\"""#));
    assert!(definition.contains("import \"constructs/statements.langmeld\";"));
    assert!(definition.contains("root \"program\";"));
    let test_source = fs::read_to_string(project.join("tests/language.rs"))
        .expect("behavior tests should be generated");
    assert!(test_source.contains("assert_eq!(output, \"Hello\\n\")"));
    assert!(test_source.contains("unknown variable `missing`"));
}

#[test]
fn init_can_include_optional_agent_instructions_and_skill() {
    let parent = TestDirectory::new();
    let project = parent.path().join("widget-language");
    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--non-interactive",
            "--agents-md",
            "--agent-skill",
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(project.join("AGENTS.md")).unwrap(),
        include_str!("../../templates/AGENTS.md")
    );
    assert_eq!(
        fs::read_to_string(project.join(".agents/skills/langmeld-language/SKILL.md")).unwrap(),
        include_str!("../../templates/langmeld-language/SKILL.md")
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("created `AGENTS.md`"));
    assert!(stdout.contains("created `.agents/skills/langmeld-language/SKILL.md`"));
}

#[test]
fn init_normalizes_directory_names_into_valid_package_defaults() {
    let parent = TestDirectory::new();
    let project = parent.path().join("_widget-language_");
    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read_to_string(project.join("Cargo.toml"))
        .expect("generated project manifest should exist");
    assert!(manifest.contains("name = \"widget-language\""));
}

#[test]
fn init_refuses_symlinked_output_directories_and_rolls_back_its_files() {
    let parent = TestDirectory::new();
    let outside = TestDirectory::new();
    let project = parent.path().join("symlinked-language");
    fs::create_dir_all(&project).expect("project directory should be created");
    std::os::unix::fs::symlink(outside.path(), project.join("src"))
        .expect("test symlink should be created");

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");

    assert!(!output.status.success());
    assert!(!project.join("Cargo.toml").exists());
    assert!(!project.join("README.md").exists());
    assert!(!outside.path().join("main.rs").exists());
}

#[test]
fn init_refuses_to_overwrite_existing_project_files() {
    let parent = TestDirectory::new();
    let project = parent.path().join("widget-language");
    fs::create_dir_all(&project).expect("project directory should be created");
    fs::write(project.join("Cargo.toml"), "user content").expect("file should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");

    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(project.join("Cargo.toml")).expect("original file should remain"),
        "user content"
    );
    assert!(!project.join("src").exists());
}

#[test]
fn help_lists_init_and_the_non_interactive_mode() {
    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["init", "--help"])
        .output()
        .expect("CLI should start");

    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("Create a starter language project"));
    assert!(help.contains("--non-interactive"));
    assert!(help.contains("--agents-md"));
    assert!(help.contains("--agent-skill"));

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["lsp", "--help"])
        .output()
        .expect("CLI should start");

    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(
        help.contains("Serve editor support for a language project or `.langmeld` definitions")
    );
    assert!(help.contains("--manifest-path"));
    assert!(help.contains("--definitions"));

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["format", "--help"])
        .output()
        .expect("CLI should start");

    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("Format a language project's `.langmeld` definitions"));
    assert!(help.contains("--check"));

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["generate", "textmate", "--help"])
        .output()
        .expect("CLI should start");

    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("Generate a lexical TextMate grammar"));
    assert!(help.contains("--scope-name"));
}

#[test]
fn init_can_scaffold_a_host_owned_rust_parser() {
    let parent = TestDirectory::new();
    let project = parent.path().join("custom-language");
    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--template",
            "custom-parser",
            "--name",
            "custom-language",
            "--language-name",
            "Custom Language",
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("cargo run --bin language-lsp"));
    assert!(project.join("Cargo.toml").is_file());
    assert!(project.join("src/lib.rs").is_file());
    assert!(project.join("src/main.rs").is_file());
    assert!(project.join("src/bin/language-lsp.rs").is_file());
    assert!(project.join("tests/parser.rs").is_file());
    assert!(!project.join("langmeld.toml").exists());
    assert!(!project.join("language").exists());
    let manifest =
        fs::read_to_string(project.join("Cargo.toml")).expect("manifest should be generated");
    assert!(manifest.contains("default-run = \"custom-language\""));
    assert!(!manifest.contains("langmeld-project"));
    assert!(manifest.contains("langmeld-lsp = "));
    let readme = fs::read_to_string(project.join("README.md")).expect("README should be generated");
    assert!(readme.contains("## License"));
    assert!(readme.contains("LICENSE-MIT"));
    let parser =
        fs::read_to_string(project.join("src/lib.rs")).expect("host parser should be generated");
    assert!(parser.contains("impl LanguageParser for SayParser"));
    assert!(parser.contains("split_inclusive('\\n')"));
    let binary =
        fs::read_to_string(project.join("src/main.rs")).expect("binary should be generated");
    assert!(binary.contains("Custom Language"));
    assert!(binary.contains("args_os()"));
    assert!(binary.contains("fs::read_to_string"));
    let lsp = fs::read_to_string(project.join("src/bin/language-lsp.rs"))
        .expect("language server entry point should be generated");
    assert!(lsp.contains("HostLanguageBackend::new(SayParser)"));
}
