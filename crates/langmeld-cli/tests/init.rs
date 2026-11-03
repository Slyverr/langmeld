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

fn run_generate_install(project: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["generate", "--install-host", "--project-root"])
        .arg(project)
        .output()
        .expect("langmeld host installation starts")
}

fn run_configure(project: &std::path::Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["configure", "--no-interactive", "--project"])
        .arg(project.join("langmeld.json"))
        .args(arguments)
        .output()
        .expect("langmeld configure starts")
}

fn run_lock(project: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["lock", "--project"])
        .arg(project.join("langmeld.json"))
        .output()
        .expect("langmeld lock starts")
}

fn run_extension(project: &std::path::Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["extension", "--project"])
        .arg(project.join("langmeld.json"))
        .args(arguments)
        .output()
        .expect("langmeld extension starts")
}

fn run_check(project: &std::path::Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(arguments)
        .current_dir(project)
        .output()
        .expect("langmeld check starts")
}

fn run_host(
    manifest: &std::path::Path,
    current_dir: &std::path::Path,
    arguments: &[&str],
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["host", "--project"])
        .arg(manifest)
        .arg("--")
        .args(arguments)
        .env("CARGO_NET_OFFLINE", "true")
        .current_dir(current_dir)
        .output()
        .expect("langmeld host starts")
}

fn run_project_host(project: &std::path::Path, arguments: &[&str]) -> std::process::Output {
    run_host(&project.join("langmeld.json"), project, arguments)
}

fn run_project_host_with_input(
    project: &std::path::Path,
    arguments: &[&str],
    input: &str,
) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["host", "--project"])
        .arg(project.join("langmeld.json"))
        .arg("--")
        .args(arguments)
        .env("CARGO_NET_OFFLINE", "true")
        .current_dir(project)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("langmeld host starts for piped prompts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(input.as_bytes())
        .expect("prompt responses are written");
    child.wait_with_output().expect("host process exits")
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
    assert!(project.join("src/main.rs").is_file());
    assert!(project.join("src/host/runtime.rs").is_file());
    let manifest = fs::read_to_string(project.join("langmeld.json"))
        .expect("init writes the project manifest");
    assert!(manifest.contains("\"language\": \"language/language.lm\""));
    assert!(manifest.contains("\"source_roots\": [\n    \"examples\"\n  ]"));
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

    let clippy = Command::new(env!("CARGO"))
        .args(["clippy", "--all-targets", "--offline", "--manifest-path"])
        .arg(project.join("Cargo.toml"))
        .args(["--", "-D", "warnings"])
        .output()
        .expect("generated project clippy starts");
    assert!(
        clippy.status.success(),
        "generated project clippy failed:\n{}\n{}",
        String::from_utf8_lossy(&clippy.stdout),
        String::from_utf8_lossy(&clippy.stderr)
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

    let checked = run_project_host(&project, &["check", "--project"]);
    assert!(
        checked.status.success(),
        "generated CLI check failed:\n{}\n{}",
        String::from_utf8_lossy(&checked.stdout),
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(String::from_utf8_lossy(&checked.stdout).contains("checked 1 source file"));

    let manifest_path = project.join("langmeld.json");
    let manifest_before = fs::read(&manifest_path).expect("starter manifest is readable");
    let unregistered = run_project_host(&project, &["configure", "--backend", "missing"]);
    assert_eq!(unregistered.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&unregistered.stderr).contains("no backends are registered"));
    assert_eq!(
        fs::read(&manifest_path).expect("starter manifest remains readable"),
        manifest_before
    );

    let direct = run_project_host(&project, &["check", "examples/hello.eventlang.lmprog"]);
    assert!(
        direct.status.success(),
        "generated CLI direct check failed:\n{}\n{}",
        String::from_utf8_lossy(&direct.stdout),
        String::from_utf8_lossy(&direct.stderr)
    );

    let run = run_project_host(&project, &["run", "--project"]);
    assert_eq!(run.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("runtime behavior is not implemented yet")
    );

    let library_path = project.join("src/lib.rs");
    let library = fs::read_to_string(&library_path).expect("runtime hook is readable");
    let library = library.replace(
        "pub fn run_sources(_sources: &[SourceFile]) -> Result<(), String> {\n    Err(\"runtime behavior is not implemented yet; add it in src/lib.rs\".to_owned())\n}",
        "pub fn run_sources(sources: &[SourceFile]) -> Result<(), String> {\n    println!(\"host executed {} source(s)\", sources.len());\n    Ok(())\n}",
    );
    fs::write(&library_path, library).expect("host runtime implementation is written");

    let executed = run_project_host(&project, &["run", "--project"]);
    assert!(
        executed.status.success(),
        "host runtime failed:\n{}\n{}",
        String::from_utf8_lossy(&executed.stdout),
        String::from_utf8_lossy(&executed.stderr)
    );
    assert!(String::from_utf8_lossy(&executed.stdout).contains("host executed 1 source(s)"));

    let library = fs::read_to_string(&library_path).expect("runtime hook remains readable");
    let library = library.replace(
        "    println!(\"host executed {} source(s)\", sources.len());\n    Ok(())",
        "    let _ = sources;\n    Err(\"interpreter rejected input\".to_owned())",
    );
    fs::write(&library_path, library).expect("host runtime error is configured");
    let failed = run_project_host(&project, &["run", "--project"]);
    assert_eq!(failed.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&failed.stderr).contains("interpreter rejected input"));
}

#[test]
fn lock_command_pins_language_definitions_and_refreshes_stale_inputs() {
    let workspace = TempWorkspace::new();
    let project = workspace.project("locked-language");
    let initialized = run_init(&[
        "--no-interactive",
        "--path",
        project.to_str().expect("project path is UTF-8"),
        "--name",
        "LockedLanguage",
    ]);
    assert!(
        initialized.status.success(),
        "project initialization failed:\n{}\n{}",
        String::from_utf8_lossy(&initialized.stdout),
        String::from_utf8_lossy(&initialized.stderr)
    );

    let locked = run_lock(&project);
    assert!(
        locked.status.success(),
        "lock generation failed:\n{}\n{}",
        String::from_utf8_lossy(&locked.stdout),
        String::from_utf8_lossy(&locked.stderr)
    );
    assert!(project.join("langmeld.lock").is_file());
    let checked = run_check(&project, &["check", "--project", "langmeld.json"]);
    assert!(checked.status.success(), "matching lock should load");

    let main_definition = project.join("language/language.lm");
    let mut contents = fs::read_to_string(&main_definition).expect("language definition reads");
    contents.push_str("\n# lock input changed\n");
    fs::write(&main_definition, contents).expect("language definition changes");
    let stale = run_check(&project, &["check", "--project", "langmeld.json"]);
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("project lockfile"));

    let refreshed = run_lock(&project);
    assert!(
        refreshed.status.success(),
        "stale lock refresh should succeed"
    );
    let checked = run_check(&project, &["check", "--project", "langmeld.json"]);
    assert!(checked.status.success(), "refreshed lock should load");
}

#[test]
fn extension_commands_manage_local_paths_and_versioned_packages() {
    let workspace = TempWorkspace::new();
    let project = workspace.project("extension-project");
    let initialized = run_init(&[
        "--no-interactive",
        "--path",
        project.to_str().expect("project path is UTF-8"),
        "--name",
        "ExtensionLanguage",
    ]);
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    fs::create_dir_all(project.join("extensions")).expect("extension directory is created");
    fs::write(
        project.join("extensions/local.lm"),
        r#"language ExtensionLanguage {
  version: "0.1.0"
  root: program
  construct note {
    syntax { default: "note {name}" }
    fields { name: identifier required }
  }
}
"#,
    )
    .expect("extension definition is written");
    fs::write(
        project.join("extensions/extra.lm"),
        r#"language ExtensionLanguage {
  version: "0.1.0"
  root: program
  construct extra {
    syntax { default: "extra {name}" }
    fields { name: identifier required }
  }
}
"#,
    )
    .expect("dependent extension definition is written");

    let added_path = run_extension(&project, &["add", "--path-only", "extensions/local.lm"]);
    assert!(
        added_path.status.success(),
        "{}",
        String::from_utf8_lossy(&added_path.stderr)
    );
    let listed = run_extension(&project, &["list"]);
    assert!(listed.status.success());
    assert!(String::from_utf8_lossy(&listed.stdout).contains("extensions/local.lm"));
    assert!(
        run_check(&project, &["check", "--project", "langmeld.json"])
            .status
            .success()
    );

    let removed_path = run_extension(&project, &["remove", "--path", "extensions/local.lm"]);
    assert!(
        removed_path.status.success(),
        "{}",
        String::from_utf8_lossy(&removed_path.stderr)
    );
    let added_package = run_extension(
        &project,
        &[
            "add",
            "--id",
            "local.notes",
            "--version",
            "1.0.0",
            "--path",
            "extensions/local.lm",
            "--language-version",
            "^0.1",
        ],
    );
    assert!(
        added_package.status.success(),
        "{}",
        String::from_utf8_lossy(&added_package.stderr)
    );
    let added_dependent = run_extension(
        &project,
        &[
            "add",
            "--id",
            "local.extra",
            "--version",
            "1.0.0",
            "--path",
            "extensions/extra.lm",
            "--language-version",
            "^0.1",
            "--depends",
            "local.notes@^1.0",
        ],
    );
    assert!(
        added_dependent.status.success(),
        "{}",
        String::from_utf8_lossy(&added_dependent.stderr)
    );
    let rejected_removal = run_extension(&project, &["remove", "local.notes"]);
    assert_eq!(rejected_removal.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&rejected_removal.stderr).contains("requires package"),
        "unexpected removal error: {}",
        String::from_utf8_lossy(&rejected_removal.stderr)
    );
    let updated = run_extension(
        &project,
        &[
            "update",
            "local.notes",
            "--version",
            "1.1.0",
            "--clear-dependencies",
        ],
    );
    assert!(
        updated.status.success(),
        "{}",
        String::from_utf8_lossy(&updated.stderr)
    );
    assert!(
        run_check(&project, &["check", "--project", "langmeld.json"])
            .status
            .success()
    );
    let removed_dependent = run_extension(&project, &["remove", "local.extra"]);
    assert!(removed_dependent.status.success());
    let removed_package = run_extension(&project, &["remove", "local.notes"]);
    assert!(
        removed_package.status.success(),
        "{}",
        String::from_utf8_lossy(&removed_package.stderr)
    );
}

#[test]
fn init_can_create_a_library_without_a_cli_target() {
    let temporary = TempWorkspace::new();
    let project = temporary.project("library-only");
    let output = run_init(&[
        "--no-interactive",
        "--library-only",
        "--path",
        project.to_str().unwrap(),
        "--name",
        "LibraryOnly",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!project.join("src/main.rs").exists());
    assert!(!project.join("src/configure.rs").exists());
    assert!(
        !fs::read_to_string(project.join("Cargo.toml"))
            .expect("library manifest is readable")
            .contains("inquire")
    );

    let cargo = Command::new(env!("CARGO"))
        .args(["test", "--offline", "--manifest-path"])
        .arg(project.join("Cargo.toml"))
        .output()
        .expect("library project cargo test starts");
    assert!(
        cargo.status.success(),
        "library project tests failed:\n{}\n{}",
        String::from_utf8_lossy(&cargo.stdout),
        String::from_utf8_lossy(&cargo.stderr)
    );

    let host = run_project_host(&project, &[]);
    assert_eq!(host.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&host.stderr).contains("no Rust CLI target"));
}

#[test]
fn generated_host_registry_dispatches_and_validates_workspace_backends() {
    let temporary = TempWorkspace::new();
    let project = temporary.project("backend-consumer");
    let initialized = run_init(&[
        "--no-interactive",
        "--path",
        project.to_str().unwrap(),
        "--name",
        "BuildHost",
        "--no-agent-skill",
    ]);
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );

    let backend_path = project.join("src/host/backend.rs");
    let backend = fs::read_to_string(&backend_path).expect("host registry is readable");
    let backend = backend.replace(
        "    Vec::new()\n}",
        "    vec![\n        RegisteredBackend::Workspace(Box::new(TestBackend)),\n        RegisteredBackend::Single(Box::new(SingleTestBackend)),\n    ]\n}",
    );
    let implementations = r#"
struct TestBackend;

impl langmeld::language::WorkspaceArtifactBackend for TestBackend {
    fn descriptor(&self) -> langmeld::language::BackendDescriptor {
        langmeld::language::BackendDescriptor {
            id: "workspace-test".to_owned(),
            formats: vec![langmeld::language::BackendFormat {
                id: "text".to_owned(),
                media_type: "text/plain".to_owned(),
                extension: "txt".to_owned(),
            }, langmeld::language::BackendFormat {
                id: "json".to_owned(),
                media_type: "application/json".to_owned(),
                extension: "json".to_owned(),
            }],
            required_ir: vec!["langmeld.workspace".to_owned()],
            supports_source_maps: true,
        }
    }

    fn settings(&self) -> Vec<langmeld::language::BackendSetting> {
        vec![
            langmeld::language::BackendSetting::choice("mode", "Mode", ["strict", "release"])
                .default_value("strict"),
        ]
    }

    fn compile_workspace(
        &self,
        _inputs: &langmeld::language::WorkspaceBackendInputs<'_>,
        options: &langmeld::language::BackendOptions,
    ) -> Result<langmeld::language::CompilationArtifact, Vec<langmeld::core::Diagnostic>> {
        let mode = options.settings.get("mode").map_or("unset", String::as_str);
        let target = options.target.as_deref().unwrap_or("default");
        let media_type = if options.format == "json" {
            "application/json"
        } else {
            "text/plain"
        };
        Ok(langmeld::language::CompilationArtifact {
            format: options.format.clone(),
            media_type: media_type.to_owned(),
            bytes: format!("compiled-workspace mode={mode} target={target}").into_bytes(),
            source_map: if options.source_maps {
                vec![langmeld::language::SourceMapEntry::in_source(
                    langmeld::core::Span::new(0, 3),
                    langmeld::core::Span::new(0, 3),
                    0,
                )]
            } else {
                Vec::new()
            },
        })
    }
}

struct SingleTestBackend;

impl langmeld::language::ArtifactBackend for SingleTestBackend {
    fn descriptor(&self) -> langmeld::language::BackendDescriptor {
        langmeld::language::BackendDescriptor {
            id: "single-test".to_owned(),
            formats: vec![langmeld::language::BackendFormat {
                id: "text".to_owned(),
                media_type: "text/plain".to_owned(),
                extension: "txt".to_owned(),
            }],
            required_ir: vec!["langmeld.program".to_owned()],
            supports_source_maps: false,
        }
    }

    fn compile(
        &self,
        _inputs: &langmeld::language::BackendInputs<'_>,
        _options: &langmeld::language::BackendOptions,
    ) -> Result<langmeld::language::CompilationArtifact, Vec<langmeld::core::Diagnostic>> {
        Ok(langmeld::language::CompilationArtifact {
            format: "text".to_owned(),
            media_type: "text/plain".to_owned(),
            bytes: b"compiled-single".to_vec(),
            source_map: Vec::new(),
        })
    }
}
"#;
    fs::write(&backend_path, format!("{backend}\n{implementations}"))
        .expect("workspace and single backends are registered");

    fs::write(
        project.join("tests/backend_registry.rs"),
        r#"use std::collections::BTreeMap;
use buildhost_language::{host::backend::{BackendRegistry, compile_registered_backend}, parser};
use langmeld::core::SourceFile;
use langmeld::language::BackendOptions;

#[test]
fn registered_backend_returns_a_validated_workspace_artifact() {
    let registry = BackendRegistry::from_project().expect("backend registry validates");
    assert_eq!(registry.settings("workspace-test").unwrap()[0].key, "mode");
    let source = SourceFile::new("example.buildhost", "program {\n  print \"hello\"\n}\n");
    let parsed = parser().expect("host parser initializes").parse(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let files = [(&source, &parsed.program)];
    let artifact = compile_registered_backend(
        "workspace-test",
        &files,
        BTreeMap::new(),
        &BackendOptions::new("text"),
    ).expect("registered backend compiles the workspace");
    assert_eq!(
        artifact.bytes,
        b"compiled-workspace mode=strict target=default"
    );
    assert_eq!(artifact.media_type, "text/plain");
    let single = compile_registered_backend(
        "single-test",
        &files,
        BTreeMap::new(),
        &BackendOptions::new("text"),
    ).expect("single-source backend compiles one program");
    assert_eq!(single.bytes, b"compiled-single");
    let multiple = [(&source, &parsed.program), (&source, &parsed.program)];
    assert!(compile_registered_backend(
        "single-test",
        &multiple,
        BTreeMap::new(),
        &BackendOptions::new("text"),
    ).unwrap_err().to_string().contains("accepts one source file"));
    assert!(compile_registered_backend(
        "missing",
        &files,
        BTreeMap::new(),
        &BackendOptions::new("text"),
    ).unwrap_err().to_string().contains("available backends: single-test, workspace-test"));
}
"#,
    )
    .expect("backend registry test is written");
    let check = Command::new(env!("CARGO"))
        .args(["test", "--offline", "--manifest-path"])
        .arg(project.join("Cargo.toml"))
        .args(["--test", "backend_registry"])
        .output()
        .expect("generated backend registry test starts");
    assert!(
        check.status.success(),
        "generated backend registry test failed:\n{}\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let manifest_path = project.join("langmeld.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("project manifest is readable"))
            .expect("project manifest is valid JSON");
    manifest["build"] = serde_json::json!({
        "backend": "workspace-test",
        "source_maps": true
    });
    fs::write(
        &manifest_path,
        format!("{}\n", serde_json::to_string_pretty(&manifest).unwrap()),
    )
    .expect("backend defaults are saved");

    let source_configuration_before = serde_json::json!({
        "language": manifest["language"].clone(),
        "sources": manifest["sources"].clone(),
        "source_roots": manifest["source_roots"].clone(),
        "source_extensions": manifest["source_extensions"].clone(),
    });
    let configured = run_project_host(
        &project,
        &[
            "configure",
            "--format",
            "text",
            "--target",
            "configured-target",
            "--source-maps",
            "--set",
            "mode=release",
        ],
    );
    assert!(
        configured.status.success(),
        "backend configuration failed:\n{}\n{}",
        String::from_utf8_lossy(&configured.stdout),
        String::from_utf8_lossy(&configured.stderr)
    );
    let configured_manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("configured manifest is readable"))
            .expect("configured manifest is valid JSON");
    assert_eq!(
        serde_json::json!({
            "language": configured_manifest["language"].clone(),
            "sources": configured_manifest["sources"].clone(),
            "source_roots": configured_manifest["source_roots"].clone(),
            "source_extensions": configured_manifest["source_extensions"].clone(),
        }),
        source_configuration_before
    );
    assert_eq!(configured_manifest["build"]["backend"], "workspace-test");
    assert_eq!(configured_manifest["build"]["format"], "text");
    assert_eq!(configured_manifest["build"]["target"], "configured-target");
    assert_eq!(configured_manifest["build"]["source_maps"], true);
    assert_eq!(configured_manifest["build"]["settings"]["mode"], "release");

    let unchanged = fs::read(&manifest_path).expect("configured manifest bytes are readable");
    let invalid = run_project_host(&project, &["configure", "--set", "mode=fast"]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("does not accept `fast`"));
    assert_eq!(
        fs::read(&manifest_path).expect("invalid configuration leaves manifest readable"),
        unchanged
    );

    for arguments in [
        vec!["configure", "--backend", "missing-backend"],
        vec!["configure", "--format", "missing-format"],
        vec!["configure", "--target", ""],
        vec!["configure", "--backend", "single-test", "--source-maps"],
        vec!["configure", "--set", "unknown=value"],
        vec![
            "configure",
            "--set",
            "mode=release",
            "--clear-setting",
            "mode",
        ],
        vec!["configure", "--set", "mode=release", "--set", "mode=strict"],
        vec!["configure", "--clear-settings", "--set", "mode=release"],
        vec!["configure", "--interactive", "--set", "mode=release"],
        vec!["configure", "--clear", "--set", "mode=release"],
    ] {
        let invalid = run_project_host(&project, &arguments);
        assert_eq!(invalid.status.code(), Some(2));
        assert_eq!(
            fs::read(&manifest_path).expect("invalid options leave manifest readable"),
            unchanged,
            "configuration options {arguments:?} must not change the manifest"
        );
    }

    let interactive =
        run_project_host_with_input(&project, &["configure", "--interactive"], "1\n1\n\n\n\n");
    assert!(
        interactive.status.success(),
        "interactive backend configuration failed:\n{}\n{}",
        String::from_utf8_lossy(&interactive.stdout),
        String::from_utf8_lossy(&interactive.stderr)
    );
    let interactive_output = String::from_utf8_lossy(&interactive.stdout);
    assert!(interactive_output.contains("2. json"));
    assert!(interactive_output.contains("value: strict"));
    assert!(interactive_output.contains("value: release"));

    let built = run_project_host(&project, &["build", "--project"]);
    assert!(
        built.status.success(),
        "generated build failed:\n{}\n{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    assert_eq!(
        fs::read(project.join("target/buildhost.txt")).expect("artifact is written"),
        b"compiled-workspace mode=release target=configured-target"
    );
    let map: serde_json::Value = serde_json::from_slice(
        &fs::read(project.join("target/buildhost.txt.map.json")).expect("source map is written"),
    )
    .expect("source map is valid JSON");
    assert_eq!(map["version"], 1);
    assert_eq!(map["mappings"][0]["source_index"], 0);

    let direct = run_project_host(
        &project,
        &[
            "build",
            "examples/hello.buildhost.lmprog",
            "--backend",
            "single-test",
            "--format",
            "text",
            "--output",
            "target/single.txt",
        ],
    );
    assert!(
        direct.status.success(),
        "direct-source build failed:\n{}\n{}",
        String::from_utf8_lossy(&direct.stdout),
        String::from_utf8_lossy(&direct.stderr)
    );
    assert_eq!(
        fs::read(project.join("target/single.txt")).expect("direct artifact is written"),
        b"compiled-single"
    );

    let collision = run_project_host(
        &project,
        &[
            "build",
            "--project",
            "--output",
            "examples/hello.buildhost.lmprog",
        ],
    );
    assert_eq!(collision.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&collision.stderr).contains("collides with source"));

    let cleared_fields = run_project_host(
        &project,
        &[
            "configure",
            "--clear-format",
            "--clear-target",
            "--clear-source-maps",
            "--clear-setting",
            "mode",
        ],
    );
    assert!(
        cleared_fields.status.success(),
        "individual defaults could not be cleared:\n{}\n{}",
        String::from_utf8_lossy(&cleared_fields.stdout),
        String::from_utf8_lossy(&cleared_fields.stderr)
    );
    let partially_cleared: serde_json::Value = serde_json::from_slice(
        &fs::read(&manifest_path).expect("partially cleared manifest is readable"),
    )
    .expect("partially cleared manifest is valid JSON");
    assert_eq!(partially_cleared["build"]["backend"], "workspace-test");
    assert!(partially_cleared["build"].get("format").is_none());
    assert!(partially_cleared["build"].get("target").is_none());
    assert!(partially_cleared["build"].get("source_maps").is_none());
    assert!(partially_cleared["build"].get("settings").is_none());

    let switched = run_project_host(&project, &["configure", "--backend", "single-test"]);
    assert!(
        switched.status.success(),
        "backend switch failed:\n{}\n{}",
        String::from_utf8_lossy(&switched.stdout),
        String::from_utf8_lossy(&switched.stderr)
    );
    let switched_manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("switched manifest is readable"))
            .expect("switched manifest is valid JSON");
    assert_eq!(switched_manifest["build"]["backend"], "single-test");
    assert!(switched_manifest["build"].get("target").is_none());
    assert!(switched_manifest["build"].get("settings").is_none());

    let cleared = run_project_host(&project, &["configure", "--clear"]);
    assert!(cleared.status.success());
    let cleared_manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("cleared manifest is readable"))
            .expect("cleared manifest is valid JSON");
    assert!(cleared_manifest.get("build").is_none());
    assert_eq!(
        serde_json::json!({
            "language": cleared_manifest["language"].clone(),
            "sources": cleared_manifest["sources"].clone(),
            "source_roots": cleared_manifest["source_roots"].clone(),
            "source_extensions": cleared_manifest["source_extensions"].clone(),
        }),
        source_configuration_before
    );
}

#[test]
fn configure_validates_candidate_before_writing_and_supports_noninteractive_updates() {
    let temporary = TempWorkspace::new();
    let project = temporary.project("configured");
    let init = run_init(&[
        "--no-interactive",
        "--path",
        project.to_str().unwrap(),
        "--name",
        "Configured",
    ]);
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let manifest_path = project.join("langmeld.json");
    let initial = fs::read(&manifest_path).expect("initial manifest is readable");

    let no_options = run_configure(&project, &[]);
    assert_eq!(no_options.status.code(), Some(2));
    assert_eq!(fs::read(&manifest_path).unwrap(), initial);

    let invalid = run_configure(&project, &["--source-root", "missing"]);
    assert_eq!(invalid.status.code(), Some(2));
    assert_eq!(fs::read(&manifest_path).unwrap(), initial);

    let source = "examples/hello.configured.lmprog";
    let updated = run_configure(&project, &["--source", source]);
    assert!(
        updated.status.success(),
        "configure failed:\n{}\n{}",
        String::from_utf8_lossy(&updated.stdout),
        String::from_utf8_lossy(&updated.stderr)
    );
    let configured: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("updated manifest is readable"))
            .expect("updated manifest is valid JSON");
    assert_eq!(configured["sources"], serde_json::json!([source]));
    assert_eq!(configured["source_roots"][0], "examples");

    let before_clear = fs::read(&manifest_path).expect("updated manifest is readable");
    let clear_all = run_configure(&project, &["--clear-sources", "--clear-source-roots"]);
    assert_eq!(clear_all.status.code(), Some(2));
    assert_eq!(fs::read(&manifest_path).unwrap(), before_clear);
}

#[test]
fn configure_interactive_blank_answers_preserve_existing_values() {
    let temporary = TempWorkspace::new();
    let project = temporary.project("interactive-configure");
    let init = run_init(&[
        "--no-interactive",
        "--path",
        project.to_str().unwrap(),
        "--name",
        "Interactive",
    ]);
    assert!(init.status.success());
    let manifest_path = project.join("langmeld.json");
    let original: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("manifest is readable"))
            .expect("manifest is valid JSON");
    let mut child = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["configure", "--interactive", "--project"])
        .arg(&manifest_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("interactive configure starts");
    child
        .stdin
        .take()
        .expect("configure stdin is piped")
        .write_all(b"\n\n\n")
        .expect("blank defaults are written");
    let output = child.wait_with_output().expect("configure exits");
    assert!(
        output.status.success(),
        "configure failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let updated: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("updated manifest is readable"))
            .expect("updated manifest is valid JSON");
    assert_eq!(updated, original);
}

#[test]
fn check_and_validate_report_workspace_diagnostics_with_stable_exit_codes() {
    let temporary = TempWorkspace::new();
    let project = temporary.project("checked");
    let init = run_init(&[
        "--no-interactive",
        "--path",
        project.to_str().unwrap(),
        "--name",
        "Checked",
    ]);
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let first = project.join("examples/second.checked.lmprog");
    fs::copy(project.join("examples/hello.checked.lmprog"), &first)
        .expect("second project source is copied");

    let valid = run_check(
        &project,
        &["validate", "--project", "langmeld.json", "--format", "json"],
    );
    assert!(
        valid.status.success(),
        "validation failed:\n{}\n{}",
        String::from_utf8_lossy(&valid.stdout),
        String::from_utf8_lossy(&valid.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&valid.stdout)
        .expect("workspace validation writes machine-readable JSON");
    assert_eq!(json["valid"], true);
    assert_eq!(json["files"].as_array().unwrap().len(), 2);

    fs::write(&first, "not a valid program\n").expect("invalid source is written");
    let invalid = run_check(
        &project,
        &["check", "--project", "langmeld.json", "--format", "json"],
    );
    assert_eq!(invalid.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&invalid.stdout)
        .expect("invalid source still produces diagnostics JSON");
    assert_eq!(json["valid"], false);
    assert!(json["files"].as_array().unwrap().iter().any(|file| {
        file["source"]
            .as_str()
            .is_some_and(|source| source.ends_with("second.checked.lmprog"))
            && !file["diagnostics"].as_array().unwrap().is_empty()
    }));

    let direct = run_check(
        &project,
        &[
            "check",
            "--language",
            "language/language.lm",
            "--source",
            "examples/hello.checked.lmprog",
        ],
    );
    assert!(
        direct.status.success(),
        "{}",
        String::from_utf8_lossy(&direct.stderr)
    );
    assert!(String::from_utf8_lossy(&direct.stdout).contains("no issues found"));
}

#[test]
fn host_trampoline_selects_default_binary_forwards_arguments_and_exit_status() {
    let temporary = TempWorkspace::new();
    let project = temporary.project("host-trampoline");
    fs::create_dir_all(project.join("src/bin")).expect("binary source directory is created");
    fs::create_dir_all(project.join("nested")).expect("nested working directory is created");
    fs::write(
        project.join("Cargo.toml"),
        r#"[package]
name = "host-trampoline"
version = "0.1.0"
edition = "2024"
default-run = "runner"

[[bin]]
name = "runner"
path = "src/main.rs"

[[bin]]
name = "other"
path = "src/bin/other.rs"
"#,
    )
    .expect("Cargo manifest is written");
    fs::write(
        project.join("src/main.rs"),
        r#"fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments.first().is_some_and(|argument| argument == "exit-7") {
        std::process::exit(7);
    }
    println!("runner:{}", arguments.join("|"));
}
"#,
    )
    .expect("default host binary is written");
    fs::write(
        project.join("src/bin/other.rs"),
        "fn main() { println!(\"wrong binary\"); }\n",
    )
    .expect("secondary host binary is written");
    fs::write(
        project.join("langmeld.json"),
        r#"{"schema_version":1,"language":"language.lm","sources":["source.calc"]}"#,
    )
    .expect("project manifest is written");

    let forwarded = run_host(
        std::path::Path::new("../langmeld.json"),
        &project.join("nested"),
        &["--flag", "two words"],
    );
    assert!(
        forwarded.status.success(),
        "host trampoline failed:\n{}\n{}",
        String::from_utf8_lossy(&forwarded.stdout),
        String::from_utf8_lossy(&forwarded.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&forwarded.stdout),
        "runner:--flag|two words\n"
    );

    let exit = run_host(std::path::Path::new("langmeld.json"), &project, &["exit-7"]);
    assert_eq!(exit.status.code(), Some(7));
}

#[test]
fn host_trampoline_reports_projects_without_a_cli_target() {
    let temporary = TempWorkspace::new();
    let project = temporary.project("no-host-cli");
    fs::create_dir_all(project.join("src")).expect("library source directory is created");
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"no-host-cli\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("library Cargo manifest is written");
    fs::write(project.join("src/lib.rs"), "pub fn value() {}\n")
        .expect("library source is written");
    fs::write(project.join("langmeld.json"), "{}\n").expect("manifest is written");
    let missing = run_host(std::path::Path::new("langmeld.json"), &project, &[]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("no Rust CLI target"));
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
        .write_all(b"y\nn\n")
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
    let input = format!("{}\nPromptedLang\nn\ny\n", project.display());
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
    assert!(transcript.contains("Include a command-line host?"));
    assert!(transcript.contains("PROJECT PREVIEW"));
    assert!(!project.join("src/main.rs").exists());
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
        &constructs_path,
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

    let installed = run_generate_install(&project);
    assert!(
        installed.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&installed.stdout),
        String::from_utf8_lossy(&installed.stderr)
    );
    let library_path = project.join("src/lib.rs");
    let library = fs::read_to_string(&library_path).expect("host module is wired");
    assert_eq!(library.matches("pub mod host;").count(), 1);
    assert!(project.join("src/host/runtime.rs").is_file());

    let checked = run_project_host(&project, &["check", "--project"]);
    assert!(
        checked.status.success(),
        "host callbacks did not support project check:\n{}\n{}",
        String::from_utf8_lossy(&checked.stdout),
        String::from_utf8_lossy(&checked.stderr)
    );

    let parser_path = project.join("src/host/parser.rs");
    let parser = fs::read_to_string(&parser_path).expect("custom parser is installed");
    fs::write(
        &parser_path,
        parser.replace(
            "    Ok(None)\n",
            "    // User-owned parser behavior.\n    Ok(None)\n",
        ),
    )
    .expect("user callback body is edited");
    let analysis_path = project.join("src/host/analysis.rs");
    let analysis = fs::read_to_string(&analysis_path).expect("analysis callbacks are installed");
    fs::write(
        &analysis_path,
        analysis.replace(
            "// Add semantic checks here.",
            "// User-owned semantic behavior.",
        ),
    )
    .expect("user analysis callback body is edited");

    let definition = fs::read_to_string(&definition_path).expect("definition remains readable");
    fs::write(
        &definition_path,
        definition.replace(
            "  behavior semantic_policy: semantic_analysis\n",
            "  behavior semantic_policy: semantic_analysis\n  behavior late_policy: semantic_analysis\n",
        ),
    )
    .expect("a new behavior callback is declared");
    let refreshed = run_generate_install(&project);
    assert!(
        refreshed.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&refreshed.stdout),
        String::from_utf8_lossy(&refreshed.stderr)
    );
    let parser = fs::read_to_string(&parser_path).expect("parser callback survives refresh");
    assert!(parser.contains("User-owned parser behavior."));
    let analysis = fs::read_to_string(&analysis_path).expect("analysis callbacks survive refresh");
    assert!(analysis.contains("User-owned semantic behavior."));
    assert!(analysis.contains("host_semantic_callback_late_policy"));
    assert_eq!(
        fs::read_to_string(&library_path)
            .expect("library remains wired")
            .matches("pub mod host;")
            .count(),
        1
    );

    let idempotent = run_generate_install(&project);
    assert!(
        idempotent.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&idempotent.stdout),
        String::from_utf8_lossy(&idempotent.stderr)
    );
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
    assert_eq!(host::analysis::semantic_behavior_hooks(&spec).unwrap().len(), 2);
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

    let installed_parser = fs::read(&parser_path).expect("installed parser remains readable");
    let generated_parser_path = host_generated.join("parser.rs");
    let generated_parser =
        fs::read(&generated_parser_path).expect("generated parser remains readable");
    let constructs =
        fs::read_to_string(&constructs_path).expect("construct module remains readable");
    fs::write(
        &constructs_path,
        constructs.replace("    custom_parser: statement_parser\n", ""),
    )
    .expect("removed callback is reflected in the language definition");
    let rejected = run_generate_install(&project);
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("were removed"),
        "{}",
        String::from_utf8_lossy(&rejected.stderr)
    );
    assert_eq!(
        fs::read(&parser_path).expect("installed parser is preserved"),
        installed_parser
    );
    assert_eq!(
        fs::read(&generated_parser_path).expect("generated output is preserved"),
        generated_parser
    );
}

#[cfg(unix)]
#[test]
fn generate_install_host_refuses_symlink_before_project_writes() {
    use std::os::unix::fs::symlink;

    let temporary = TempWorkspace::new();
    let project = temporary.project("symlink-host");
    let initialized = run_init(&[
        "--no-interactive",
        "--path",
        project.to_str().unwrap(),
        "--name",
        "SymlinkHost",
        "--no-agent-skill",
    ]);
    assert!(initialized.status.success());

    let outside_host = temporary.project("outside-host");
    fs::create_dir(&outside_host).expect("external target directory is created");
    fs::write(outside_host.join("keep.txt"), "keep").expect("external marker is written");
    fs::remove_dir_all(project.join("src/host")).expect("generated host tree is removed");
    symlink(&outside_host, project.join("src/host")).expect("host path symlink is created");
    let library_before = fs::read(project.join("src/lib.rs")).expect("library is readable");
    let tooling_before = fs::read(project.join(".langmeld/generated/host/README.md"))
        .expect("host artifact is readable");

    let rejected = run_generate_install(&project);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("refusing links"));
    assert_eq!(
        fs::read(project.join("src/lib.rs")).unwrap(),
        library_before
    );
    assert_eq!(
        fs::read(project.join(".langmeld/generated/host/README.md")).unwrap(),
        tooling_before
    );
    assert_eq!(
        fs::read_to_string(outside_host.join("keep.txt")).unwrap(),
        "keep"
    );
}
