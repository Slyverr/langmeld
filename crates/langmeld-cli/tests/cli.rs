use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("langmeld-cli-test-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).expect("test directory should be created");
        Self(path)
    }

    fn path(&self) -> &PathBuf {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

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
            "Widget Script",
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(project.join("Cargo.toml").is_file());
    assert!(project.join("langmeld.toml").is_file());
    assert!(project.join("src/main.rs").is_file());
    assert!(project.join("language/language.lm").is_file());
    assert!(project.join("language/constructs/statements.lm").is_file());
    let manifest =
        fs::read_to_string(project.join("Cargo.toml")).expect("manifest should be generated");
    assert!(manifest.contains("[workspace]"));
    let definition = fs::read_to_string(project.join("language/language.lm"))
        .expect("definition should be generated");
    assert!(definition.starts_with("language \"Widget Script\""));
    assert!(definition.contains("import \"constructs/statements.lm\";"));
}

#[test]
fn check_uses_the_project_manifest_and_reports_the_language_name() {
    let parent = TestDirectory::new();
    let project = parent.path().join("widget-language");
    fs::create_dir_all(project.join("language")).expect("language directory should be created");
    fs::write(
        project.join("langmeld.toml"),
        "format = 1\ndefinition = \"language/language.lm\"\n",
    )
    .expect("manifest should be written");
    fs::write(
        project.join("language/language.lm"),
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
}

#[test]
fn check_reports_definition_diagnostics_with_source_locations() {
    let parent = TestDirectory::new();
    let project = parent.path().join("invalid-language");
    fs::create_dir_all(project.join("language")).expect("language directory should be created");
    fs::write(
        project.join("langmeld.toml"),
        "format = 1\ndefinition = \"language/language.lm\"\n",
    )
    .expect("manifest should be written");
    fs::write(
        project.join("language/language.lm"),
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
    assert!(error.contains("language/language.lm:1:"));
    assert!(error.contains("LM-DEF-"));
}

#[cfg(unix)]
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
    assert!(project.join("Cargo.toml").is_file());
    assert!(project.join("src/main.rs").is_file());
    assert!(!project.join("langmeld.toml").exists());
    assert!(!project.join("language").exists());
    let manifest =
        fs::read_to_string(project.join("Cargo.toml")).expect("manifest should be generated");
    assert!(!manifest.contains("langmeld-project"));
    let parser =
        fs::read_to_string(project.join("src/main.rs")).expect("host parser should be generated");
    assert!(parser.contains("impl LanguageParser for SayParser"));
    assert!(parser.contains("Custom Language"));
}
