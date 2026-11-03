use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TempWorkspace(PathBuf);

impl TempWorkspace {
    fn new() -> Self {
        loop {
            let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("langmeld-init-test-{}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot create test workspace: {error}"),
            }
        }
    }

    fn project(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run_init(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["init"])
        .args(arguments)
        .output()
        .expect("langmeld binary starts")
}

fn run_generate(project: &std::path::Path, check: bool) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_langmeld"));
    command.args(["generate", "--project-root"]).arg(project);
    if check {
        command.arg("--check");
    }
    command.output().expect("langmeld generate starts")
}

#[test]
fn init_creates_modular_project_and_generated_tooling_that_builds_and_runs() {
    let temporary = TempWorkspace::new();
    let project = temporary.project("created/nested/starter");
    let output = run_init(&[
        "--no-interactive",
        "--path",
        project.to_str().unwrap(),
        "--name",
        "EventLang",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(project.join("AGENTS.md").is_file());
    assert!(!project.join(".agents/skills/langmeld/SKILL.md").exists());
    assert!(project.join("language/language.lm").is_file());
    assert!(project.join("language/constructs/program.lm").is_file());
    assert!(project.join(".langmeld/generated/metadata.json").is_file());
    assert!(project.join(".langmeld/generated/rust/ast.rs").is_file());
    assert!(
        fs::read_to_string(project.join(".gitignore"))
            .expect("gitignore exists")
            .contains("/.langmeld/")
    );

    let cargo = Command::new(env!("CARGO"))
        .args(["test", "--offline", "--manifest-path"])
        .arg(project.join("Cargo.toml"))
        .output()
        .expect("generated project cargo test starts");
    assert!(
        cargo.status.success(),
        "generated project tests failed:\n{}\n{}",
        String::from_utf8_lossy(&cargo.stdout),
        String::from_utf8_lossy(&cargo.stderr)
    );

    let example = Command::new(env!("CARGO"))
        .args(["run", "--offline", "--manifest-path"])
        .arg(project.join("Cargo.toml"))
        .args(["--example", "hello"])
        .output()
        .expect("generated example starts");
    assert!(
        example.status.success(),
        "generated example failed:\n{}\n{}",
        String::from_utf8_lossy(&example.stdout),
        String::from_utf8_lossy(&example.stderr)
    );
    assert!(String::from_utf8_lossy(&example.stdout).contains("Parsed 1 top-level construct"));
}

#[test]
fn interactive_cancel_does_not_create_files() {
    let temporary = TempWorkspace::new();
    let project = temporary.project("cancelled");
    let mut child = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            "--interactive",
            "--path",
            project.to_str().unwrap(),
            "--name",
            "Cancelled",
            "--agents",
            "--no-agent-skill",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("langmeld interactive process starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(b"n\n")
        .expect("cancel answer is written");
    let output = child.wait_with_output().expect("interactive process exits");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!project.exists());
    assert!(String::from_utf8_lossy(&output.stdout).contains("cancelled"));
}

#[test]
fn piped_interactive_setup_collects_path_and_name_before_creating_project() {
    let temporary = TempWorkspace::new();
    let project = temporary.project("prompted");
    let mut child = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["init", "--interactive", "--no-agents", "--no-agent-skill"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("langmeld interactive process starts");
    let input = format!("{}\nPromptedLang\ny\n", project.display());
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(input.as_bytes())
        .expect("interactive answers are written");
    let output = child.wait_with_output().expect("interactive process exits");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(project.join("Cargo.toml").is_file());
    let transcript = String::from_utf8_lossy(&output.stdout);
    assert!(transcript.contains("Project directory"));
    assert!(transcript.contains("Language name"));
    assert!(transcript.contains("PROJECT PREVIEW"));
}

#[test]
fn init_refuses_a_nonempty_target_without_touching_existing_files() {
    let temporary = TempWorkspace::new();
    let project = temporary.project("occupied");
    fs::create_dir(&project).expect("target directory creates");
    fs::write(project.join("keep.txt"), "keep me").expect("marker writes");
    let output = run_init(&[
        "--no-interactive",
        "--path",
        project.to_str().unwrap(),
        "--name",
        "Occupied",
    ]);
    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(project.join("keep.txt")).expect("existing marker remains"),
        "keep me"
    );
}

#[test]
fn generate_check_is_read_only_and_generation_replaces_stale_derived_files() {
    let temporary = TempWorkspace::new();
    let project = temporary.project("generated");
    let initialized = run_init(&[
        "--no-interactive",
        "--path",
        project.to_str().unwrap(),
        "--name",
        "Generated",
        "--no-agent-skill",
    ]);
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    let check = run_generate(&project, true);
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );

    let stale = project.join(".langmeld/generated/stale.txt");
    fs::write(&stale, "stale").expect("stale artifact writes");
    let check = run_generate(&project, true);
    assert!(!check.status.success());
    assert_eq!(
        fs::read_to_string(&stale).expect("check is read-only"),
        "stale"
    );

    let regenerate = run_generate(&project, false);
    assert!(
        regenerate.status.success(),
        "{}",
        String::from_utf8_lossy(&regenerate.stderr)
    );
    assert!(!stale.exists());
    assert!(run_generate(&project, true).status.success());
}
