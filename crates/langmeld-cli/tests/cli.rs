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
    assert!(help.contains("Start an LSP server for a declarative language project"));
    assert!(help.contains("--manifest-path"));

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
fn lsp_command_loads_the_manifest_and_serves_scoped_completion() {
    let parent = TestDirectory::new();
    let project = parent.path();
    write_widget_project(project);

    let mut child = ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_langmeld"))
            .args([
                "lsp",
                "--manifest-path",
                project.join("langmeld.toml").to_str().unwrap(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("CLI should start"),
    );
    let mut stdin = child.0.stdin.take().unwrap();
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
