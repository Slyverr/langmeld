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
                let value = line.splitn(2, |byte| *byte == b':').nth(1).unwrap();
                content_length = Some(
                    std::str::from_utf8(value.trim_ascii())
                        .unwrap()
                        .parse::<usize>()
                        .unwrap(),
                );
            }
        }
        let mut body = vec![0; content_length.expect("content length header exists")];
        output.read_exact(&mut body).expect("message body reads");
        messages.push(serde_json::from_slice(&body).expect("message is JSON"));
    }
}

#[test]
fn launched_lsp_resolves_typescript_call_sites_through_langmeld_references() {
    let bun = std::env::var_os("LANGMELD_BUN").unwrap_or_else(|| "bun".into());
    if Command::new(bun).arg("--version").output().is_err() {
        eprintln!("skipping Bun integration test: executable was not found");
        return;
    }
    let uri = "file:///main.ts";
    let source = format!(
        "{}\nfunction unfinished(value: string): string {{ return formatGreeting(\n",
        include_str!("../main.ts")
    );
    let call_line = source
        .lines()
        .position(|line| line.contains("return formatGreeting"))
        .expect("sample contains a local TypeScript call");
    let call_character = source
        .lines()
        .nth(call_line)
        .and_then(|line| line.find("formatGreeting"))
        .expect("call identifier has a source column");
    let requests = [
        json!({"jsonrpc":"2.0", "id":1, "method":"initialize", "params":{}}),
        json!({"jsonrpc":"2.0", "method":"textDocument/didOpen", "params":{"textDocument":{"uri":uri,"version":1,"text":source}}}),
        json!({"jsonrpc":"2.0", "id":2, "method":"textDocument/definition", "params":{"textDocument":{"uri":uri},"position":{"line":call_line,"character":call_character+2}}}),
        json!({"jsonrpc":"2.0", "id":3, "method":"shutdown"}),
        json!({"jsonrpc":"2.0", "method":"exit"}),
    ];
    let mut child = Command::new(env!("CARGO_BIN_EXE_typescript-source-lsp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("TypeScript source LSP host starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(&frame(&requests))
        .expect("requests are written");
    let output = child.wait_with_output().expect("LSP host exits");
    assert!(
        output.status.success(),
        "LSP host failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let messages = decode(Cursor::new(output.stdout));
    let definition = messages
        .iter()
        .find(|message| message["id"] == 2)
        .expect("definition response exists");

    assert_eq!(
        definition["result"][0]["uri"], uri,
        "responses: {messages:#?}"
    );
    assert!(messages.iter().any(|message| {
        message["method"] == "textDocument/publishDiagnostics"
            && message["params"]["diagnostics"]
                .as_array()
                .is_some_and(|diagnostics| diagnostics.iter().any(|item| item["code"] == "TS1001"))
    }));
    let declaration_line = source
        .lines()
        .position(|line| line.contains("function formatGreeting("))
        .expect("sample declares the function");
    assert_eq!(
        definition["result"][0]["range"]["start"]["line"],
        declaration_line
    );
}

#[test]
fn launched_project_lsp_navigates_through_named_and_default_imports() {
    let bun = std::env::var_os("LANGMELD_BUN").unwrap_or_else(|| "bun".into());
    if Command::new(bun).arg("--version").output().is_err() {
        eprintln!("skipping Bun integration test: executable was not found");
        return;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("project")
        .canonicalize()
        .expect("TypeScript project fixture exists");
    let main_path = root.join("main.ts");
    let target_path = root.join("greetings.ts");
    let alternate_path = root.join("other.ts");
    let main_uri = format!("file://{}", main_path.display());
    let target_uri = format!("file://{}", target_path.display());
    let alternate_uri = format!("file://{}", alternate_path.display());
    let source = std::fs::read_to_string(&main_path).expect("project main reads");
    let greet_line = source
        .lines()
        .position(|line| line.contains("greet("))
        .expect("project main contains the imported call");
    let greet_character = source
        .lines()
        .nth(greet_line)
        .and_then(|line| line.find("greet"))
        .expect("import alias has a source position");
    let alternate_line = source
        .lines()
        .position(|line| line.contains("alternate("))
        .expect("project main contains the second imported call");
    let alternate_character = source
        .lines()
        .nth(alternate_line)
        .and_then(|line| line.find("alternate"))
        .expect("second import alias has a source position");
    let invalid_source = source.replace("./greetings", "./missing");
    let requests = [
        json!({"jsonrpc":"2.0", "id":1, "method":"initialize", "params":{}}),
        json!({"jsonrpc":"2.0", "method":"textDocument/didOpen", "params":{"textDocument":{"uri":main_uri,"version":1,"text":source}}}),
        json!({"jsonrpc":"2.0", "id":2, "method":"textDocument/definition", "params":{"textDocument":{"uri":main_uri},"position":{"line":greet_line,"character":greet_character+1}}}),
        json!({"jsonrpc":"2.0", "id":4, "method":"textDocument/definition", "params":{"textDocument":{"uri":main_uri},"position":{"line":alternate_line,"character":alternate_character+1}}}),
        json!({"jsonrpc":"2.0", "method":"textDocument/didChange", "params":{"textDocument":{"uri":main_uri,"version":2},"contentChanges":[{"text":invalid_source}]}}),
        json!({"jsonrpc":"2.0", "id":3, "method":"shutdown"}),
        json!({"jsonrpc":"2.0", "method":"exit"}),
    ];
    let mut child = Command::new(env!("CARGO_BIN_EXE_typescript-source-lsp"))
        .args([
            "--project-root",
            root.to_str().expect("fixture path is UTF-8"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("TypeScript project LSP host starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(&frame(&requests))
        .expect("requests are written");
    let output = child.wait_with_output().expect("LSP host exits");
    assert!(
        output.status.success(),
        "LSP host failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let messages = decode(Cursor::new(output.stdout));
    let definition = messages
        .iter()
        .find(|message| message["id"] == 2)
        .expect("cross-file definition response exists");
    assert_eq!(definition["result"][0]["uri"], target_uri);
    let alternate_definition = messages
        .iter()
        .find(|message| message["id"] == 4)
        .expect("second definition response exists");
    assert_eq!(
        alternate_definition["result"][0]["uri"], alternate_uri,
        "responses: {messages:#?}"
    );
    assert_eq!(definition["result"][0]["range"]["start"]["line"], 0);
    assert!(messages.iter().any(|message| {
        message["method"] == "textDocument/publishDiagnostics"
            && message["params"]["diagnostics"]
                .as_array()
                .is_some_and(|diagnostics| diagnostics.iter().any(|item| item["code"] == "TS1004"))
    }));
}
