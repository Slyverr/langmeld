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
