use std::{
    fs,
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use serde_json::{Value, json};

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

fn write_widget_project(project: &Path) {
    fs::create_dir_all(project.join("language/constructs")).unwrap();
    fs::write(
        project.join("langmeld.toml"),
        "format = 1\ndefinition = \"language/language.langmeld\"\n",
    )
    .unwrap();
    fs::write(
        project.join("language/language.langmeld"),
        "language \"Widget\" { import \"constructs/statements.langmeld\"; construct \"program\" { syntax { literal \"program\"; } block \"{\" \"}\" { allow \"print\"; } } }",
    )
    .unwrap();
    fs::write(
        project.join("language/constructs/statements.langmeld"),
        "construct \"print\" { syntax { literal \"print\"; identifier \"name\"; literal \";\"; } }",
    )
    .unwrap();
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn send_lsp_message(writer: &mut impl Write, message: Value) {
    let body = serde_json::to_vec(&message).expect("message should serialize");
    write!(writer, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
    writer.write_all(&body).unwrap();
    writer.flush().unwrap();
}

fn read_lsp_message(reader: &mut impl BufRead) -> io::Result<Value> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "language server closed stdout",
            ));
        }
        if line == "\r\n" {
            break;
        }
        if let Some((header, value)) = line.split_once(':')
            && header.eq_ignore_ascii_case("Content-Length")
        {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            );
        }
    }

    let mut body = vec![
        0;
        content_length.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length header")
        })?
    ];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn receive_lsp_message(receiver: &mpsc::Receiver<io::Result<Value>>) -> Value {
    receiver
        .recv_timeout(Duration::from_secs(3))
        .expect("language server should respond before timeout")
        .expect("language server should send a valid LSP message")
}

fn canonical_file_uri(uri: &str) -> String {
    let path = url::Url::parse(uri)
        .expect("file URI should be valid")
        .to_file_path()
        .expect("URI should identify a local file");
    url::Url::from_file_path(fs::canonicalize(path).expect("file should exist"))
        .expect("canonical path should form a file URI")
        .to_string()
}

fn start_lsp(
    args: &[&str],
    current_directory: Option<&Path>,
) -> (
    ChildGuard,
    std::process::ChildStdin,
    mpsc::Receiver<io::Result<Value>>,
) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_langmeld"));
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(current_directory) = current_directory {
        command.current_dir(current_directory);
    }
    let mut child = ChildGuard(command.spawn().expect("CLI should start"));
    let stdin = child.0.stdin.take().unwrap();
    let stdout = child.0.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        while let Ok(message) = read_lsp_message(&mut reader) {
            if sender.send(Ok(message)).is_err() {
                break;
            }
        }
    });
    (child, stdin, receiver)
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
    assert_eq!(
        fs::read_to_string(project.join("LICENSE-MIT")).unwrap(),
        include_str!("../templates/LICENSE-MIT")
    );
    assert_eq!(
        fs::read_to_string(project.join("LICENSE-APACHE")).unwrap(),
        include_str!("../templates/LICENSE-APACHE")
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
    assert!(definition.starts_with("language \"Widget Script\""));
    assert!(definition.contains("import \"constructs/statements.langmeld\";"));
    assert!(definition.contains("root \"program\";"));
    let test_source = fs::read_to_string(project.join("tests/language.rs"))
        .expect("behavior tests should be generated");
    assert!(test_source.contains("assert_eq!(output, \"Hello\\n\")"));
    assert!(test_source.contains("unknown variable `missing`"));
}

#[test]
fn generate_handler_adds_a_stub_and_managed_registration_without_overwriting() {
    let parent = TestDirectory::new();
    let project = parent.path().join("widget-language");
    let init = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );

    let statements = project.join("language/constructs/statements.langmeld");
    let mut source = fs::read_to_string(&statements).expect("constructs should be generated");
    source.push_str(
        "\nconstruct \"trace\" { syntax { literal \"trace\"; literal \";\"; } implements \"statement\"; }\n\nconstruct \"move\" { syntax { literal \"move\"; literal \";\"; } implements \"statement\"; }\n\nconstruct \"bad/name\" { syntax { literal \"bad\"; literal \";\"; } implements \"statement\"; }\n\nconstruct \"foo-bar\" { syntax { literal \"foo\"; literal \";\"; } implements \"statement\"; }\n\nconstruct \"foo_bar\" { syntax { literal \"bar\"; literal \";\"; } implements \"statement\"; }\n",
    );
    fs::write(&statements, source).expect("new constructs should be written");

    let manifest = project.join("langmeld.toml");
    let registry_path = project.join("src/constructs/mod.rs");
    let mut registry = fs::read_to_string(&registry_path).expect("registry should be generated");
    registry.push_str("\n// User-owned source outside managed sections.\n");
    fs::write(&registry_path, registry).expect("user-owned source should be preserved");

    let existing = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
            "let",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(!existing.status.success());

    let generated = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
            "trace",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let handler = project.join("src/constructs/trace.rs");
    let handler_source = fs::read_to_string(&handler).expect("handler should be generated");
    assert!(handler_source.contains("const CONSTRUCT: &'static str = \"trace\";"));
    assert!(handler_source.contains("implement this construct's language behavior"));
    let registry_path = project.join("src/constructs/mod.rs");
    let registry = fs::read_to_string(&registry_path).expect("registry should be updated");
    assert!(registry.contains("mod trace;"));
    assert!(registry.contains("handlers.register(trace::TraceHandler)?;"));
    assert!(registry.contains("    // <langmeld:end registrations>"));
    assert!(registry.contains("// User-owned source outside managed sections."));

    let original_handler = handler_source;
    let original_registry = registry;
    let repeated = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
            "trace",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(!repeated.status.success());
    assert_eq!(fs::read_to_string(handler).unwrap(), original_handler);
    assert_eq!(
        fs::read_to_string(registry_path).unwrap(),
        original_registry
    );

    let keyword = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
            "move",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(
        keyword.status.success(),
        "{}",
        String::from_utf8_lossy(&keyword.stderr)
    );
    assert!(project.join("src/constructs/move_handler.rs").is_file());
    let registry = fs::read_to_string(project.join("src/constructs/mod.rs"))
        .expect("keyword handler should be registered");
    assert!(registry.contains("mod move_handler;"));
    assert!(registry.contains("handlers.register(move_handler::MoveHandler)?;"));

    let invalid_name = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
            "bad/name",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(!invalid_name.status.success());
    assert!(
        String::from_utf8_lossy(&invalid_name.stderr).contains("cannot map to a Rust module name")
    );

    let first_colliding_name = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
            "foo-bar",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(
        first_colliding_name.status.success(),
        "{}",
        String::from_utf8_lossy(&first_colliding_name.stderr)
    );

    let second_colliding_name = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "handler",
            "foo_bar",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(!second_colliding_name.status.success());
    assert!(String::from_utf8_lossy(&second_colliding_name.stderr).contains("already exists"));
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

#[test]
fn lsp_command_loads_the_manifest_and_serves_scoped_completion() {
    let parent = TestDirectory::new();
    let project = parent.path();
    write_widget_project(project);
    let manifest_path = project.join("langmeld.toml");
    let manifest_path = manifest_path.to_str().unwrap();
    let (mut child, mut stdin, receiver) =
        start_lsp(&["lsp", "--manifest-path", manifest_path], None);

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    let initialized = receive_lsp_message(&receiver);
    assert_eq!(initialized["id"], 1);
    assert!(initialized["result"]["capabilities"]["completionProvider"].is_object());

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":"file:///main.widget","languageId":"widget","version":1,"text":"program { pri"}
            }
        }),
    );
    let diagnostics = receive_lsp_message(&receiver);
    assert_eq!(diagnostics["method"], "textDocument/publishDiagnostics");

    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{
                "textDocument":{"uri":"file:///main.widget"},"position":{"line":0,"character":13}
            }
        }),
    );
    let completion = receive_lsp_message(&receiver);
    assert_eq!(completion["id"], 2);
    assert_eq!(completion["result"][0]["label"], "print");

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":3,"method":"shutdown","params":null}),
    );
    assert_eq!(receive_lsp_message(&receiver)["id"], 3);
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"exit","params":null}),
    );
    drop(stdin);
    assert!(child.0.wait().unwrap().success());
}

#[test]
fn lsp_definitions_mode_serves_context_aware_langmeld_completions() {
    let parent = TestDirectory::new();
    let (mut child, mut stdin, receiver) =
        start_lsp(&["lsp", "--definitions"], Some(parent.path()));

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    let initialized = receive_lsp_message(&receiver);
    assert!(initialized["result"]["capabilities"]["completionProvider"].is_object());
    assert_eq!(
        initialized["result"]["capabilities"]["documentFormattingProvider"],
        true
    );
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );

    let root_source = "lan";
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":"file:///language.langmeld","languageId":"langmeld","version":1,"text":root_source}
            }
        }),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{
                "textDocument":{"uri":"file:///language.langmeld"},"position":{"line":0,"character":3}
            }
        }),
    );
    let root_completions = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(root_completions[0]["label"], "language");
    assert_eq!(root_completions.as_array().unwrap().len(), 1);

    let nested_source = "language \"Toy\" { construct \"item\" { syntax { lit";
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":"file:///constructs.langmeld","languageId":"langmeld","version":1,"text":nested_source}
            }
        }),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","id":3,"method":"textDocument/completion","params":{
                "textDocument":{"uri":"file:///constructs.langmeld"},"position":{"line":0,"character":nested_source.len()}
            }
        }),
    );
    let nested_completions = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(nested_completions.as_array().unwrap().len(), 1);
    assert_eq!(nested_completions[0]["label"], "literal");

    let construct_source = "language \"Toy\" { construct \"item\" { comp";
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":"file:///construct-body.langmeld","languageId":"langmeld","version":1,"text":construct_source}
            }
        }),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","id":4,"method":"textDocument/completion","params":{
                "textDocument":{"uri":"file:///construct-body.langmeld"},"position":{"line":0,"character":construct_source.len()}
            }
        }),
    );
    let construct_completions = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(construct_completions.as_array().unwrap().len(), 1);
    assert_eq!(construct_completions[0]["label"], "completion");

    let choice_source = "language \"Toy\" { construct \"item\" { syntax { choice \"mode\" { ";
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":"file:///choice.langmeld","languageId":"langmeld","version":1,"text":choice_source}
            }
        }),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","id":5,"method":"textDocument/completion","params":{
                "textDocument":{"uri":"file:///choice.langmeld"},"position":{"line":0,"character":choice_source.len()}
            }
        }),
    );
    let choice_completions = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(
        choice_completions
            .as_array()
            .unwrap()
            .iter()
            .map(|candidate| candidate["label"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["completion", "value", "}"]
    );

    let kind_source = "language \"Toy\" { construct \"item\" { completion key";
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":"file:///completion-kind.langmeld","languageId":"langmeld","version":1,"text":kind_source}
            }
        }),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","id":6,"method":"textDocument/completion","params":{
                "textDocument":{"uri":"file:///completion-kind.langmeld"},"position":{"line":0,"character":kind_source.len()}
            }
        }),
    );
    let kind_completions = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(kind_completions.as_array().unwrap().len(), 1);
    assert_eq!(kind_completions[0]["label"], "keyword");
    assert_eq!(kind_completions[0]["kind"], 20);

    let format_source = "language \"Toy\"{construct \"item\"{syntax{literal \"item\";}}}";
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":"file:///format.langmeld","languageId":"langmeld","version":1,"text":format_source}
            }
        }),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","id":7,"method":"textDocument/formatting","params":{
                "textDocument":{"uri":"file:///format.langmeld"},
                "options":{"tabSize":2,"insertSpaces":true}
            }
        }),
    );
    let edits = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(edits.as_array().unwrap().len(), 1);
    assert_eq!(
        edits[0]["newText"],
        "language \"Toy\" {\n  construct \"item\" {\n    syntax {\n      literal \"item\";\n    }\n  }\n}\n"
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":8,"method":"shutdown","params":null}),
    );
    assert_eq!(receive_lsp_message(&receiver)["id"], 8);
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"exit","params":null}),
    );
    drop(stdin);
    assert!(child.0.wait().unwrap().success());
}

#[test]
fn project_definitions_lsp_uses_the_open_buffer_for_import_validation() {
    let parent = TestDirectory::new();
    let project = parent.path().join("project with spaces");
    write_widget_project(&project);
    let module = project.join("language/constructs/statements.langmeld");
    let root = project.join("language/language.langmeld");
    let module_uri = url::Url::from_file_path(&module).unwrap().to_string();
    let root_uri = url::Url::from_file_path(&root).unwrap().to_string();
    let (mut child, mut stdin, receiver) = start_lsp(&["lsp", "--definitions"], Some(&project));

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    assert_eq!(receive_lsp_message(&receiver)["id"], 1);
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{
                    "uri":module_uri,
                    "languageId":"langmeld",
                    "version":1,
                    "text":"construct \"program\" { syntax { literal \"program\"; } }"
                }
            }
        }),
    );
    let diagnostics = receive_lsp_message(&receiver);
    assert_eq!(diagnostics["method"], "textDocument/publishDiagnostics");
    assert_eq!(
        diagnostics["params"]["diagnostics"][0]["code"],
        "LM-DEF-004"
    );

    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{
                    "uri":root_uri,
                    "languageId":"langmeld",
                    "version":1,
                    "text":"language \"Widget\" { import \"constructs/statements.langmeld\"; construct \"program\" { syntax { literal \"program\"; } block \"{\" \"}\" { allow \"missing\"; } } }"
                }
            }
        }),
    );
    let diagnostics = [
        receive_lsp_message(&receiver),
        receive_lsp_message(&receiver),
    ];
    assert!(
        diagnostics
            .iter()
            .all(|message| { message["method"] == "textDocument/publishDiagnostics" })
    );
    assert!(diagnostics.iter().any(|message| {
        message["params"]["diagnostics"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item["code"] == "LM-DEF-004"))
    }));

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":3,"method":"shutdown","params":null}),
    );
    assert_eq!(receive_lsp_message(&receiver)["id"], 3);
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"exit","params":null}),
    );
    drop(stdin);
    assert!(child.0.wait().unwrap().success());
}

#[test]
fn project_definitions_lsp_completes_references_and_import_paths() {
    let parent = TestDirectory::new();
    let project = parent.path().join("project");
    write_widget_project(&project);
    let root = project.join("language/language.langmeld");
    let module = project.join("language/constructs/statements.langmeld");
    let second_module = project.join("language/constructs/alpha.langmeld");
    let root_source = "language \"Widget\" { import \"constructs/statements.langmeld\"; import \"constructs/alpha.langmeld\"; construct \"program\" { syntax { literal \"program\"; } block \"{\" \"}\" { allow \"print\"; } } }";
    let module_source = "construct \"print\" { implements \"statement\"; syntax { literal \"print\"; identifier \"name\"; literal \";\"; } }";
    let second_module_source = "construct \"zebra\" { syntax { literal \"zebra\"; } }";
    fs::write(
        parent.path().join("outside.langmeld"),
        "construct \"private\" {}",
    )
    .unwrap();
    fs::write(&root, root_source).unwrap();
    fs::write(&module, module_source).unwrap();
    fs::write(&second_module, second_module_source).unwrap();
    let root_uri = url::Url::from_file_path(fs::canonicalize(&root).unwrap())
        .unwrap()
        .to_string();
    let module_uri = url::Url::from_file_path(fs::canonicalize(&module).unwrap())
        .unwrap()
        .to_string();
    let (mut child, mut stdin, receiver) = start_lsp(&["lsp", "--definitions"], Some(&project));

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    assert_eq!(receive_lsp_message(&receiver)["id"], 1);
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    for (uri, text) in [(&root_uri, root_source), (&module_uri, module_source)] {
        send_lsp_message(
            &mut stdin,
            json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"langmeld","version":1,"text":text}}}),
        );
        let count = if uri == &root_uri { 1 } else { 2 };
        for _ in 0..count {
            assert_eq!(
                receive_lsp_message(&receiver)["method"],
                "textDocument/publishDiagnostics"
            );
        }
    }

    let allow_offset = root_source.find("allow \"").unwrap() + "allow \"".len();
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":8,"method":"textDocument/completion","params":{"textDocument":{"uri":root_uri},"position":{"line":0,"character":allow_offset}}}),
    );
    let construct_completion = receive_lsp_message(&receiver);
    let construct_labels = construct_completion["result"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["label"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(construct_labels, ["program", "print", "zebra"]);

    for (id, uri, text, prefix, label, detail, kind) in [
        (2, &root_uri, root_source, "pri", "print", "Construct", 18),
        (
            3,
            &module_uri,
            module_source,
            "sta",
            "statement",
            "Category",
            18,
        ),
        (
            4,
            &root_uri,
            root_source,
            "constructs/stat",
            "constructs/statements.langmeld",
            ".langmeld definition",
            17,
        ),
        (
            5,
            &root_uri,
            root_source,
            "con",
            "constructs/",
            "Folder",
            19,
        ),
    ] {
        let offset = text.find(prefix).unwrap() + prefix.len();
        let position = json!({"line": 0, "character": offset});
        send_lsp_message(
            &mut stdin,
            json!({"jsonrpc":"2.0","id":id,"method":"textDocument/completion","params":{"textDocument":{"uri":uri},"position":position}}),
        );
        let completion = receive_lsp_message(&receiver);
        assert_eq!(completion["result"].as_array().unwrap().len(), 1);
        assert_eq!(completion["result"][0]["label"], label);
        assert_eq!(completion["result"][0]["detail"], detail);
        assert_eq!(completion["result"][0]["kind"], kind);
        assert_eq!(
            completion["result"][0]["textEdit"]["newText"],
            format!("\"{label}\"")
        );
    }

    let outside_import =
        root_source.replace("constructs/statements.langmeld", "../../outside.langmeld");
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":root_uri,"version":2},"contentChanges":[{"text":outside_import}]}}),
    );
    for _ in 0..2 {
        assert_eq!(
            receive_lsp_message(&receiver)["method"],
            "textDocument/publishDiagnostics"
        );
    }
    let outside_prefix = "../../outside";
    let outside_offset = outside_import.find(outside_prefix).unwrap() + outside_prefix.len();
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":6,"method":"textDocument/completion","params":{"textDocument":{"uri":root_uri},"position":{"line":0,"character":outside_offset}}}),
    );
    assert!(
        receive_lsp_message(&receiver)["result"]
            .as_array()
            .unwrap()
            .is_empty(),
        "path suggestions must not expose files outside the project root"
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":7,"method":"shutdown","params":null}),
    );
    assert_eq!(receive_lsp_message(&receiver)["id"], 7);
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"exit","params":null}),
    );
    drop(stdin);
    assert!(child.0.wait().unwrap().success());
}

#[test]
fn project_definitions_lsp_refreshes_references_from_disk_and_incomplete_buffers() {
    let parent = TestDirectory::new();
    let project = parent.path().join("project");
    write_widget_project(&project);
    let root = project.join("language/language.langmeld");
    let module = project.join("language/constructs/statements.langmeld");
    let root_source = fs::read_to_string(&root).unwrap();
    let module_uri = url::Url::from_file_path(&module).unwrap().to_string();
    let root_uri = url::Url::from_file_path(&root).unwrap().to_string();
    let (mut child, mut stdin, receiver) = start_lsp(&["lsp", "--definitions"], Some(&project));

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{"workspace":{"didChangeWatchedFiles":{"dynamicRegistration":true,"relativePatternSupport":true}}}}}),
    );
    let initialized = receive_lsp_message(&receiver);
    assert_eq!(initialized["id"], 1);
    assert_eq!(
        initialized["result"]["capabilities"]["documentSymbolProvider"],
        true
    );
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    let registration = receive_lsp_message(&receiver);
    assert_eq!(registration["method"], "client/registerCapability");
    let watcher = &registration["params"]["registrations"][0]["registerOptions"]["watchers"][0];
    assert_eq!(watcher["globPattern"]["pattern"], "**/*.langmeld");
    assert_eq!(
        watcher["globPattern"]["baseUri"],
        url::Url::from_directory_path(fs::canonicalize(&project).unwrap())
            .unwrap()
            .as_str()
    );
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":registration["id"],"result":null}),
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":root_uri,"languageId":"langmeld","version":1,"text":root_source}}}),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":6,"method":"textDocument/documentSymbol","params":{"textDocument":{"uri":root_uri}}}),
    );
    let symbols = receive_lsp_message(&receiver)["result"].clone();
    let symbol_names = symbols
        .as_array()
        .unwrap()
        .iter()
        .map(|symbol| symbol["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(symbol_names.contains(&"Widget"));
    assert!(symbol_names.contains(&"constructs/statements.langmeld"));
    assert!(symbol_names.contains(&"program"));

    let completion_position = root_source.find("allow \"").unwrap() + "allow \"".len();
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{"textDocument":{"uri":root_uri},"position":{"line":0,"character":completion_position}}}),
    );
    let initial_items = receive_lsp_message(&receiver)["result"].clone();
    assert!(
        initial_items
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "print")
    );
    assert!(
        !initial_items
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "render")
    );

    let module_source = fs::read_to_string(&module).unwrap();
    fs::write(
        &module,
        format!("{module_source} construct \"render\" {{ syntax {{ literal \"render\"; }} }}"),
    )
    .unwrap();
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":module_uri,"type":2}]}}),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":3,"method":"textDocument/completion","params":{"textDocument":{"uri":root_uri},"position":{"line":0,"character":completion_position}}}),
    );
    let updated_items = receive_lsp_message(&receiver)["result"].clone();
    assert!(
        updated_items
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "render")
    );

    let incomplete_module = format!(
        "{} construct \"preview\" {{ syntax {{ literal \"preview\"; }} }} construct \"unfinished",
        fs::read_to_string(&module).unwrap()
    );
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":module_uri,"languageId":"langmeld","version":1,"text":incomplete_module}}}),
    );
    for _ in 0..2 {
        assert_eq!(
            receive_lsp_message(&receiver)["method"],
            "textDocument/publishDiagnostics"
        );
    }
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":4,"method":"textDocument/completion","params":{"textDocument":{"uri":root_uri},"position":{"line":0,"character":completion_position}}}),
    );
    let incomplete_items = receive_lsp_message(&receiver)["result"].clone();
    assert!(
        incomplete_items
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "preview"),
        "completions should include declarations from the current incomplete buffer"
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":5,"method":"shutdown","params":null}),
    );
    assert_eq!(receive_lsp_message(&receiver)["id"], 5);
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"exit","params":null}),
    );
    drop(stdin);
    assert!(child.0.wait().unwrap().success());
}

#[test]
fn project_definitions_lsp_navigates_construct_and_category_references() {
    let parent = TestDirectory::new();
    let project = parent.path().join("project");
    write_widget_project(&project);
    let root = project.join("language/language.langmeld");
    let module = project.join("language/constructs/statements.langmeld");
    let root_source = r#"language "Widget" { import "constructs/statements.langmeld"; construct "program" { syntax { literal "program"; } block "{" "}" { allow "print"; allow_category "statement"; } } }"#;
    let module_source = r#"construct "print" { syntax { literal "print"; identifier "name"; literal ";"; } implements "statement"; } construct "say" { syntax { literal "say"; } implements "statement"; }"#;
    fs::write(&root, root_source).unwrap();
    fs::write(&module, module_source).unwrap();
    let root_uri = url::Url::from_file_path(&root).unwrap().to_string();
    let module_uri = url::Url::from_file_path(&module).unwrap().to_string();
    let (mut child, mut stdin, receiver) = start_lsp(&["lsp", "--definitions"], Some(&project));

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    let initialized = receive_lsp_message(&receiver);
    assert_eq!(
        initialized["result"]["capabilities"]["definitionProvider"],
        true
    );
    assert_eq!(
        initialized["result"]["capabilities"]["referencesProvider"],
        true
    );
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":root_uri,"languageId":"langmeld","version":1,"text":root_source}}}),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );

    let construct_offset = root_source.find("allow \"print\"").unwrap() + "allow \"p".len();
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{"textDocument":{"uri":root_uri},"position":{"line":0,"character":construct_offset}}}),
    );
    let definition = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(definition.as_array().unwrap().len(), 1);
    assert_eq!(
        canonical_file_uri(definition[0]["uri"].as_str().unwrap()),
        canonical_file_uri(&module_uri)
    );
    let print_span = module_source.find("\"print\"").unwrap();
    assert_eq!(definition[0]["range"]["start"]["character"], print_span);
    assert_eq!(
        definition[0]["range"]["end"]["character"],
        print_span + "\"print\"".len()
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":3,"method":"textDocument/references","params":{"textDocument":{"uri":root_uri},"position":{"line":0,"character":construct_offset},"context":{"includeDeclaration":true}}}),
    );
    let references = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(references.as_array().unwrap().len(), 2);
    assert!(references.as_array().unwrap().iter().any(|location| {
        canonical_file_uri(location["uri"].as_str().unwrap()) == canonical_file_uri(&root_uri)
    }));
    assert!(references.as_array().unwrap().iter().any(|location| {
        canonical_file_uri(location["uri"].as_str().unwrap()) == canonical_file_uri(&module_uri)
    }));

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":module_uri,"languageId":"langmeld","version":1,"text":module_source}}}),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );
    let declaration_offset =
        module_source.find("construct \"print\"").unwrap() + "construct \"p".len();
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":6,"method":"textDocument/references","params":{"textDocument":{"uri":module_uri},"position":{"line":0,"character":declaration_offset},"context":{"includeDeclaration":false}}}),
    );
    let mut response = receive_lsp_message(&receiver);
    while response["id"] != 6 {
        response = receive_lsp_message(&receiver);
    }
    let uses = response["result"].clone();
    assert_eq!(uses.as_array().unwrap().len(), 1);
    assert_eq!(uses[0]["uri"], root_uri);

    let category_offset =
        root_source.find("allow_category \"statement\"").unwrap() + "allow_category \"s".len();
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":4,"method":"textDocument/definition","params":{"textDocument":{"uri":root_uri},"position":{"line":0,"character":category_offset}}}),
    );
    let category_definitions = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(category_definitions.as_array().unwrap().len(), 2);
    assert!(
        category_definitions
            .as_array()
            .unwrap()
            .iter()
            .all(|location| {
                canonical_file_uri(location["uri"].as_str().unwrap())
                    == canonical_file_uri(&module_uri)
            })
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":5,"method":"shutdown","params":null}),
    );
    assert_eq!(receive_lsp_message(&receiver)["id"], 5);
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"exit","params":null}),
    );
    drop(stdin);
    assert!(child.0.wait().unwrap().success());
}

#[test]
fn project_definitions_lsp_reanalyzes_all_open_buffers_and_reverts_closed_overlays() {
    let parent = TestDirectory::new();
    let project = parent.path().join("project with spaces");
    write_widget_project(&project);
    let module = project.join("language/constructs/statements.langmeld");
    let root = project.join("language/language.langmeld");
    let module_uri = url::Url::from_file_path(&module).unwrap().to_string();
    let root_uri = url::Url::from_file_path(&root).unwrap().to_string();
    let (mut child, mut stdin, receiver) = start_lsp(&["lsp", "--definitions"], Some(&project));

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    assert_eq!(receive_lsp_message(&receiver)["id"], 1);
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );

    let root_allows_missing = "language \"Widget\" { import \"constructs/statements.langmeld\"; construct \"program\" { syntax { literal \"program\"; } block \"{\" \"}\" { allow \"missing\"; } } }";
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":root_uri,"languageId":"langmeld","version":1,"text":root_allows_missing}}}),
    );
    let root_diagnostics = receive_lsp_message(&receiver);
    assert_eq!(root_diagnostics["params"]["uri"], root_uri);
    assert_eq!(
        root_diagnostics["params"]["diagnostics"][0]["code"],
        "LM-DEF-004"
    );

    let module_defines_missing = "construct \"missing\" { syntax { literal \"missing\"; } }";
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":module_uri,"languageId":"langmeld","version":1,"text":module_defines_missing}}}),
    );
    let opened_module_messages = [
        receive_lsp_message(&receiver),
        receive_lsp_message(&receiver),
    ];
    let root_after_module_open = opened_module_messages
        .iter()
        .find(|message| message["params"]["uri"] == root_uri)
        .expect("opening an imported buffer republishes root diagnostics");
    assert!(
        root_after_module_open["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let root_allows_print = "language \"Widget\" { import \"constructs/statements.langmeld\"; construct \"program\" { syntax { literal \"program\"; } block \"{\" \"}\" { allow \"print\"; } } }";
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":root_uri,"version":2},"contentChanges":[{"text":root_allows_print}]}}),
    );
    let changed_root_messages = [
        receive_lsp_message(&receiver),
        receive_lsp_message(&receiver),
    ];
    let root_after_change = changed_root_messages
        .iter()
        .find(|message| message["params"]["uri"] == root_uri)
        .expect("changing the root republishes diagnostics for every open file");
    assert_eq!(
        root_after_change["params"]["diagnostics"][0]["code"],
        "LM-DEF-004"
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":module_uri}}}),
    );
    let closed_module_messages = [
        receive_lsp_message(&receiver),
        receive_lsp_message(&receiver),
    ];
    let root_after_module_close = closed_module_messages
        .iter()
        .find(|message| message["params"]["uri"] == root_uri)
        .expect("closing an imported buffer republishes remaining project diagnostics");
    assert!(
        root_after_module_close["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}),
    );
    assert_eq!(receive_lsp_message(&receiver)["id"], 2);
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"exit","params":null}),
    );
    drop(stdin);
    assert!(child.0.wait().unwrap().success());
}

#[test]
fn generate_construct_adds_import_syntax_and_registered_handler() {
    let parent = TestDirectory::new();
    let project = parent.path().join("widget-language");
    let init = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "init",
            project.to_str().expect("temporary path is UTF-8"),
            "--non-interactive",
        ])
        .output()
        .expect("CLI should start");
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );

    let manifest = project.join("langmeld.toml");
    let generated = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "construct",
            "trace",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let root = fs::read_to_string(project.join("language/main.langmeld"))
        .expect("root definition should be readable");
    assert!(root.contains("import \"constructs/trace.langmeld\";"));
    let syntax = fs::read_to_string(project.join("language/constructs/trace.langmeld"))
        .expect("construct syntax should be generated");
    assert!(syntax.contains("literal \"trace\";"));
    assert!(syntax.contains("implements \"statement\";"));
    let handler = fs::read_to_string(project.join("src/constructs/trace.rs"))
        .expect("construct handler should be generated");
    assert!(handler.contains("const CONSTRUCT: &'static str = \"trace\";"));
    let registry = fs::read_to_string(project.join("src/constructs/mod.rs"))
        .expect("handler registry should be readable");
    assert!(registry.contains("mod trace;"));
    assert!(registry.contains("handlers.register(trace::TraceHandler)?;"));

    let checked = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "check",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );

    let repeated = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "construct",
            "trace",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(!repeated.status.success());
    assert_eq!(
        fs::read_to_string(project.join("language/main.langmeld")).unwrap(),
        root
    );

    let keyword = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "construct",
            "loop",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(
        keyword.status.success(),
        "{}",
        String::from_utf8_lossy(&keyword.stderr)
    );
    assert!(project.join("language/constructs/loop.langmeld").is_file());
    assert!(project.join("src/constructs/loop_handler.rs").is_file());
    let registry = fs::read_to_string(project.join("src/constructs/mod.rs"))
        .expect("keyword handler should be registered");
    assert!(registry.contains("mod loop_handler;"));
    assert!(registry.contains("handlers.register(loop_handler::LoopHandler)?;"));

    let invalid_keyword = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "construct",
            "trace-item",
            "--manifest-path",
            manifest.to_str().expect("temporary path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(!invalid_keyword.status.success());
    assert!(
        !project
            .join("language/constructs/trace_item.langmeld")
            .exists()
    );
}

#[test]
fn generate_textmate_uses_imported_definitions_and_writes_json_to_stdout() {
    let parent = TestDirectory::new();
    let project = parent.path();
    write_widget_project(project);

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "textmate",
            "--manifest-path",
            project.join("langmeld.toml").to_str().unwrap(),
            "--scope-name",
            "source.widget",
            "--file-type",
            "widget",
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let grammar: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(grammar["name"], "Widget");
    assert_eq!(grammar["scopeName"], "source.widget");
    assert_eq!(grammar["fileTypes"], json!(["widget"]));
    assert!(
        grammar["patterns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|pattern| {
                pattern["name"] == "keyword.other.widget"
                    && pattern["match"].as_str().unwrap().contains("print")
            })
    );
}

#[test]
fn generate_tree_sitter_writes_a_grammar_js_source_file_to_stdout() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/tree-sitter-language/langmeld.toml");

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args([
            "generate",
            "tree-sitter",
            "--manifest-path",
            manifest.to_str().unwrap(),
            "--grammar-name",
            "widget_script",
        ])
        .output()
        .expect("CLI should start");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let grammar = String::from_utf8(output.stdout).expect("grammar source should be UTF-8");
    assert!(grammar.contains("module.exports = grammar({"));
    assert!(grammar.contains("name: \"widget_script\""));
    assert!(grammar.contains("source_file: $ => repeat(choice($.construct_"));
    assert!(grammar.contains("identifier: _ =>"));
    assert!(grammar.contains("optional(seq(\"to\", field(\"target\", $.identifier)))"));
    assert!(grammar.contains("repeat(seq(\"with\", field(\"label\", $.string)))"));
    assert!(grammar.contains("choice(seq(\"say\","));
    assert!(grammar.contains("field(\"value\", $.token)"));
    assert!(
        grammar.contains("token: $ => choice($.identifier, $.integer, $.string, $.punctuation)")
    );
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
