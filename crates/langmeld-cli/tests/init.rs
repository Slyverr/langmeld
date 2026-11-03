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

#[test]
fn generate_emits_host_callbacks_that_compile_in_a_project_consumer() {
    let temporary = TempWorkspace::new();
    let project = temporary.project("host-consumer");
    let initialized = run_init(&[
        "--no-interactive",
        "--path",
        project.to_str().unwrap(),
        "--name",
        "HostConsumer",
        "--no-agent-skill",
    ]);
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );

    let definition_path = project.join("language/language.lm");
    let definition = fs::read_to_string(&definition_path).expect("starter definition is written");
    let definition = definition.replace(
        "  root: program\n",
        "  root: program\n  behavior semantic_policy: semantic_analysis\n  behavior workspace_names: symbol_resolution\n  behavior workspace_types: workspace_type_analysis\n",
    );
    fs::write(&definition_path, definition).expect("analysis callbacks are declared");
    let constructs_path = project.join("language/constructs/program.lm");
    let constructs = fs::read_to_string(&constructs_path).expect("starter constructs are written");
    fs::write(
        constructs_path,
        constructs.replace(
            "  construct statement {\n",
            "  construct statement {\n    custom_parser: statement_parser\n",
        ),
    )
    .expect("custom parser callback is declared");

    let generated = run_generate(&project, false);
    assert!(
        generated.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&generated.stdout),
        String::from_utf8_lossy(&generated.stderr)
    );
    let host_generated = project.join(".langmeld/generated/host");
    let parser = fs::read_to_string(host_generated.join("parser.rs"))
        .expect("custom parser scaffold is generated");
    assert!(parser.contains(
        "CustomConstructParser::new(\"statement_parser\", host_parser_callback_statement_parser)"
    ));
    let analysis = fs::read_to_string(host_generated.join("analysis.rs"))
        .expect("analysis scaffold is generated");
    assert!(analysis.contains("host_semantic_callback_semantic_policy"));
    assert!(analysis.contains("host_resolver_callback_workspace_names"));
    assert!(analysis.contains("host_workspace_callback_workspace_types"));
    assert!(
        run_generate(&project, true).status.success(),
        "host scaffolds are part of deterministic check output"
    );

    copy_generated_tree(&host_generated, &project.join("src/host"));
    let library_path = project.join("src/lib.rs");
    let mut library = fs::read_to_string(&library_path).expect("starter library is written");
    library.push_str("\npub mod host;\n");
    fs::write(library_path, library).expect("generated host modules are exposed to the crate");
    let tests = project.join("tests");
    fs::create_dir_all(&tests).expect("generated consumer tests directory is created");
    fs::write(
        tests.join("generated_host.rs"),
        r#"use hostconsumer_language::{host, load_language};
use langmeld::core::SourceFile;

#[test]
fn generated_host_binds_declared_callbacks_and_parses_source() {
    let spec = load_language().expect("language definition loads");
    let parser = host::runtime::configured_parser(spec.clone()).expect("callbacks bind");
    let source = SourceFile::new("input", "program {\n  print \"Hello\"\n}\n");
    let parsed = parser.parse(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.program.nodes.len(), 1);
    assert_eq!(host::analysis::semantic_behavior_hooks(&spec).unwrap().len(), 1);
    assert_eq!(host::analysis::resolver_behavior_hooks(&spec).unwrap().len(), 1);
    assert_eq!(host::analysis::workspace_behavior_hooks(&spec).unwrap().len(), 1);
}
"#,
    )
    .expect("generated runtime consumer test is written");
    let check = Command::new(env!("CARGO"))
        .args([
            "test",
            "--test",
            "generated_host",
            "--offline",
            "--manifest-path",
        ])
        .arg(project.join("Cargo.toml"))
        .output()
        .expect("generated consumer cargo tests start");
    assert!(
        check.status.success(),
        "generated host consumer failed:\n{}\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
}

fn copy_generated_tree(source: &std::path::Path, destination: &std::path::Path) {
    fs::create_dir_all(destination).expect("generated consumer directory is created");
    for entry in fs::read_dir(source).expect("generated host tree is readable") {
        let entry = entry.expect("generated host entry is readable");
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry
            .file_type()
            .expect("generated entry type is readable")
            .is_dir()
        {
            copy_generated_tree(&source_path, &destination_path);
        } else {
            fs::copy(&source_path, &destination_path).expect("generated host file is copied");
        }
    }
}
