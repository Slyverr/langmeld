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
        "format = 1\ndefinition = \"language/language.lm\"\n",
    )
    .unwrap();
    fs::write(
        project.join("language/language.lm"),
        "language \"Widget\" { import \"constructs/statements.lm\"; construct \"program\" { syntax { literal \"program\"; } block \"{\" \"}\" { allow \"print\"; } } }",
    )
    .unwrap();
    fs::write(
        project.join("language/constructs/statements.lm"),
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

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["lsp", "--help"])
        .output()
        .expect("CLI should start");

    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("Serve editor support for a language project or `.lm` definitions"));
    assert!(help.contains("--manifest-path"));
    assert!(help.contains("--definitions"));

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["format", "--help"])
        .output()
        .expect("CLI should start");

    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("Format one Langmeld `.lm` definition file"));
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
    let definition = parent.path().join("language.lm");
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

    let invalid_definition = parent.path().join("incomplete.lm");
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
fn lsp_definitions_mode_serves_context_aware_lm_completions() {
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
                "textDocument":{"uri":"file:///language.lm","languageId":"lm","version":1,"text":root_source}
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
                "textDocument":{"uri":"file:///language.lm"},"position":{"line":0,"character":3}
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
                "textDocument":{"uri":"file:///constructs.lm","languageId":"lm","version":1,"text":nested_source}
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
                "textDocument":{"uri":"file:///constructs.lm"},"position":{"line":0,"character":nested_source.len()}
            }
        }),
    );
    let nested_completions = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(nested_completions.as_array().unwrap().len(), 1);
    assert_eq!(nested_completions[0]["label"], "literal");

    let format_source = "language \"Toy\"{construct \"item\"{syntax{literal \"item\";}}}";
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":"file:///format.lm","languageId":"lm","version":1,"text":format_source}
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
            "jsonrpc":"2.0","id":4,"method":"textDocument/formatting","params":{
                "textDocument":{"uri":"file:///format.lm"},
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
    assert!(project.join("src/lib.rs").is_file());
    assert!(project.join("src/main.rs").is_file());
    assert!(project.join("tests/parser.rs").is_file());
    assert!(!project.join("langmeld.toml").exists());
    assert!(!project.join("language").exists());
    let manifest =
        fs::read_to_string(project.join("Cargo.toml")).expect("manifest should be generated");
    assert!(!manifest.contains("langmeld-project"));
    let parser =
        fs::read_to_string(project.join("src/lib.rs")).expect("host parser should be generated");
    assert!(parser.contains("impl LanguageParser for SayParser"));
    assert!(parser.contains("split_inclusive('\\n')"));
    let binary =
        fs::read_to_string(project.join("src/main.rs")).expect("binary should be generated");
    assert!(binary.contains("Custom Language"));
}
