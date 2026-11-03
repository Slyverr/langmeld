use std::{
    io::{BufRead, Cursor, Read, Write},
    process::{Command, Stdio},
};

use serde_json::{Value, json};

fn frame(messages: &[Value]) -> Vec<u8> {
    let mut input = Vec::new();
    for message in messages {
        let body = serde_json::to_vec(message).expect("message serializes");
        input.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
        input.extend_from_slice(&body);
    }
    input
}

fn decode(mut output: Cursor<Vec<u8>>) -> Vec<Value> {
    let mut messages = Vec::new();
    loop {
        let mut content_length = None;
        loop {
            let mut line = Vec::new();
            output.read_until(b'\n', &mut line).expect("header reads");
            if line.is_empty() {
                return messages;
            }
            if line == b"\r\n" || line == b"\n" {
                break;
            }
            if line
                .split(|byte| *byte == b':')
                .next()
                .is_some_and(|name| name.eq_ignore_ascii_case(b"content-length"))
            {
                let value = line
                    .splitn(2, |byte| *byte == b':')
                    .nth(1)
                    .expect("header value");
                content_length = Some(
                    std::str::from_utf8(value.trim_ascii())
                        .expect("header is ASCII")
                        .parse::<usize>()
                        .expect("content length is an integer"),
                );
            }
        }
        let mut body = vec![0; content_length.expect("content length header exists")];
        output.read_exact(&mut body).expect("message body reads");
        messages.push(serde_json::from_slice(&body).expect("message is JSON"));
    }
}

fn run_lsp(args: &[&str], messages: &[Value]) -> (Vec<Value>, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_python-source-lsp"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Python source LSP host starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(&frame(messages))
        .expect("LSP messages are written");
    let output = child.wait_with_output().expect("LSP process exits");
    assert!(
        output.status.success(),
        "LSP host failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    (
        decode(Cursor::new(output.stdout)),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn launched_lsp_navigates_local_calls_while_recovering_malformed_syntax() {
    let uri = "file:///main.py";
    let source = "def target() -> str:\n    return 'ok'\n\ndef caller() -> str:\n    return target()\n\ndef unfinished( -> str:\n    return 'broken'\n";
    let call_line = source
        .lines()
        .position(|line| line.contains("return target"))
        .expect("source has a local function call");
    let call_character = source
        .lines()
        .nth(call_line)
        .and_then(|line| line.find("target"))
        .expect("call identifier has a position");
    let messages = [
        json!({"jsonrpc":"2.0", "id":1, "method":"initialize", "params":{}}),
        json!({"jsonrpc":"2.0", "method":"textDocument/didOpen", "params":{"textDocument":{"uri":uri,"version":1,"text":source}}}),
        json!({"jsonrpc":"2.0", "id":2, "method":"textDocument/definition", "params":{"textDocument":{"uri":uri},"position":{"line":call_line,"character":call_character+2}}}),
        json!({"jsonrpc":"2.0", "id":3, "method":"shutdown"}),
        json!({"jsonrpc":"2.0", "method":"exit"}),
    ];
    let (responses, _) = run_lsp(&[], &messages);
    let definition = responses
        .iter()
        .find(|message| message["id"] == 2)
        .expect("definition response exists");
    assert_eq!(definition["result"][0]["uri"], uri);
    assert!(responses.iter().any(|message| {
        message["method"] == "textDocument/publishDiagnostics"
            && message["params"]["diagnostics"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item["code"] == "PS1001"))
    }));
    assert_eq!(definition["result"][0]["range"]["start"]["line"], 0);
}

#[test]
fn launched_project_lsp_resolves_relative_imports_and_updates_diagnostics() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("project")
        .canonicalize()
        .expect("Python project fixture exists");
    let main_path = root.join("main.py");
    let target_path = root.join("greetings.py");
    let main_uri = format!("file://{}", main_path.display());
    let target_uri = format!("file://{}", target_path.display());
    let source = std::fs::read_to_string(&main_path).expect("project main reads");
    let call_line = source
        .lines()
        .position(|line| line.contains("greet_user(name)"))
        .expect("project source calls the imported alias");
    let call_character = source
        .lines()
        .nth(call_line)
        .and_then(|line| line.find("greet_user"))
        .expect("import alias call has a source position");
    let invalid_source = source.replace(".greetings", ".missing");
    let messages = [
        json!({"jsonrpc":"2.0", "id":1, "method":"initialize", "params":{}}),
        json!({"jsonrpc":"2.0", "method":"textDocument/didOpen", "params":{"textDocument":{"uri":main_uri,"version":1,"text":source}}}),
        json!({"jsonrpc":"2.0", "id":2, "method":"textDocument/definition", "params":{"textDocument":{"uri":main_uri},"position":{"line":call_line,"character":call_character+2}}}),
        json!({"jsonrpc":"2.0", "method":"textDocument/didChange", "params":{"textDocument":{"uri":main_uri,"version":2},"contentChanges":[{"text":invalid_source}]}}),
        json!({"jsonrpc":"2.0", "id":3, "method":"shutdown"}),
        json!({"jsonrpc":"2.0", "method":"exit"}),
    ];
    let root_arg = root.to_str().expect("fixture path is UTF-8");
    let (responses, _) = run_lsp(&["--project-root", root_arg], &messages);
    let definition = responses
        .iter()
        .find(|message| message["id"] == 2)
        .expect("definition response exists");
    assert_eq!(definition["result"][0]["uri"], target_uri);
    assert!(responses.iter().any(|message| {
        message["method"] == "textDocument/publishDiagnostics"
            && message["params"]["diagnostics"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item["code"] == "PS1004"))
    }));
}
