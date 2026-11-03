use std::{
    fs,
    io::{BufRead, Cursor, Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

use serde_json::{Value, json};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TempLanguage(PathBuf);

impl TempLanguage {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("langmeld-lsp-protocol-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).expect("temporary language directory is created");
        fs::write(
            path.join("language.lm"),
            r#"language Demo {
  version: "0.1.0"
  root: program
  construct program {
    syntax { default: "program {" }
    block: [statement]
  }
  construct statement {
    syntax { default: "print {message}" }
    fields { message: text required }
  }
}
"#,
        )
        .expect("language definition is written");
        Self(path.join("language.lm"))
    }
}

impl Drop for TempLanguage {
    fn drop(&mut self) {
        if let Some(parent) = self.0.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }
}

fn frame(value: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(value).expect("message serializes");
    let mut framed = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    framed.extend(body);
    framed
}

fn read_frames(bytes: &[u8]) -> Vec<Value> {
    let mut reader = Cursor::new(bytes);
    let mut messages = Vec::new();
    loop {
        let mut line = Vec::new();
        reader
            .read_until(b'\n', &mut line)
            .expect("response header reads");
        if line.is_empty() {
            break;
        }
        let header = String::from_utf8(line).expect("header is UTF-8");
        let length = header
            .strip_prefix("Content-Length: ")
            .expect("content length header")
            .trim()
            .parse::<usize>()
            .expect("content length is numeric");
        let mut separator = Vec::new();
        reader
            .read_until(b'\n', &mut separator)
            .expect("header separator reads");
        let mut body = vec![0; length];
        reader.read_exact(&mut body).expect("response body reads");
        messages.push(serde_json::from_slice(&body).expect("response body is JSON"));
    }
    messages
}

#[test]
fn child_process_initializes_and_publishes_diagnostics_for_open_documents() {
    let language = TempLanguage::new();
    let uri = format!(
        "file://{}",
        language.0.with_file_name("broken.demo").display()
    );
    let messages = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "demo",
                    "version": 1,
                    "text": "program {\nunknown\n}\n"
                }
            }
        }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": null }),
        json!({ "jsonrpc": "2.0", "method": "exit", "params": null }),
    ];
    let mut input = Vec::new();
    for message in &messages {
        input.extend(frame(message));
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_langmeld-lsp"))
        .args(["--language"])
        .arg(&language.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("language server starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(&input)
        .expect("LSP requests are written");
    let output = child.wait_with_output().expect("language server exits");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = read_frames(&output.stdout);
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0]["id"], 1);
    assert_eq!(
        responses[0]["result"]["capabilities"]["textDocumentSync"]["change"],
        0
    );
    assert_eq!(responses[1]["method"], "textDocument/publishDiagnostics");
    assert!(
        !responses[1]["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(responses[1]["params"]["version"], 1);
    assert_eq!(responses[2]["id"], 2);
}
