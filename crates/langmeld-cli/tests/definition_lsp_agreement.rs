use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

use langmeld_lsp::serve_definitions_project;
use serde_json::{Value, json};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "langmeld-cli-definition-lsp-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(path.join("language/extensions"))
            .expect("definition project directories are created");
        fs::write(
            path.join("language/language.langmeld"),
            "language ReloadDemo {\n  version: \"0.1.0\"\n  root: program\n  import: \"extensions/extra.langmeld\"\n}\n",
        )
        .expect("root definition is written");
        fs::write(
            path.join("language/extensions/extra.langmeld"),
            extension("0.1.0"),
        )
        .expect("extension definition is written");
        Self(path)
    }

    fn root_definition(&self) -> PathBuf {
        self.0.join("language/language.langmeld")
    }

    fn extension_definition(&self) -> PathBuf {
        self.0.join("language/extensions/extra.langmeld")
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn extension(version: &str) -> String {
    format!(
        "language ReloadDemo {{\n  version: \"{version}\"\n  root: program\n  construct program {{\n    syntax {{ default: \"program {{\" }}\n    block: [note]\n  }}\n  construct note {{\n    syntax {{ default: \"note\" }}\n  }}\n}}\n"
    )
}

fn run_generate(project: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .args(["generate", "--project-root"])
        .arg(project)
        .output()
        .expect("langmeld generate starts")
}

fn run_lsp(root: &Path, open_path: &Path, text: &str) -> Vec<Value> {
    let messages = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": file_uri(open_path),
                "languageId": "langmeld-definition",
                "version": 1,
                "text": text
            }}
        }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": null }),
        json!({ "jsonrpc": "2.0", "method": "exit", "params": null }),
    ];
    let mut input = Vec::new();
    for message in &messages {
        let body = serde_json::to_vec(message).expect("JSON-RPC message serializes");
        input.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        input.extend(body);
    }

    let mut output = Vec::new();
    serve_definitions_project(Cursor::new(input), &mut output, root)
        .expect("definition language server processes the fixture");
    read_frames(&output)
}

fn file_uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

fn read_frames(mut input: &[u8]) -> Vec<Value> {
    let mut messages = Vec::new();
    while !input.is_empty() {
        let header_end = input
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("JSON-RPC response has a header separator");
        let header = std::str::from_utf8(&input[..header_end]).expect("header is UTF-8");
        let content_length = header
            .lines()
            .find_map(|line| {
                line.strip_prefix("Content-Length: ")
                    .and_then(|length| length.parse::<usize>().ok())
            })
            .expect("response has a content length");
        input = &input[header_end + 4..];
        let (body, remaining) = input.split_at(content_length);
        messages.push(serde_json::from_slice(body).expect("response body is JSON"));
        input = remaining;
    }
    messages
}

fn has_definition_error(messages: &[Value]) -> bool {
    messages.iter().any(|message| {
        message["method"] == "textDocument/publishDiagnostics"
            && message["params"]["diagnostics"]
                .as_array()
                .is_some_and(|diagnostics| {
                    diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic["severity"] == 1)
                })
    })
}

#[test]
fn cli_generation_and_lm_lsp_agree_on_on_disk_definition_validity() {
    let project = TempProject::new();
    let root = project.root_definition();
    let extension_path = project.extension_definition();
    let valid = fs::read_to_string(&extension_path).expect("extension is read");

    let cli = run_generate(&project.0);
    assert!(
        cli.status.success(),
        "valid project was rejected by CLI:\n{}\n{}",
        String::from_utf8_lossy(&cli.stdout),
        String::from_utf8_lossy(&cli.stderr)
    );
    let lsp = run_lsp(&root, &extension_path, &valid);
    assert!(
        !has_definition_error(&lsp),
        "valid project has LSP errors: {lsp:?}"
    );

    let invalid = extension("0.2.0");
    fs::write(&extension_path, &invalid).expect("incompatible extension is written");
    let cli = run_generate(&project.0);
    assert!(
        !cli.status.success(),
        "CLI accepted an incompatible extension"
    );
    let lsp = run_lsp(&root, &extension_path, &invalid);
    assert!(
        has_definition_error(&lsp),
        "LSP accepted an incompatible extension"
    );
    assert!(
        String::from_utf8_lossy(&cli.stderr).contains("different language name, version, or root")
    );
}
