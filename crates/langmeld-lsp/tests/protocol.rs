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

    fn write_symbol_language(&self) {
        fs::write(
            &self.0,
            r#"language Symbols {
  version: "0.1.0"
  root: program
  construct program {
    syntax { default: "program {namespace} {" }
    fields { namespace: identifier required }
    block: [function, usage]
  }
  construct function {
    syntax { default: "fn {name} -> {result}" }
    fields {
      name: identifier required
      result: identifier required
    }
    symbol {
      kind: function
      name: name
      scope: program
      scope_name: namespace
    }
  }
  construct usage {
    syntax { default: "use {target}" }
    fields { target: ref<function> required }
  }
}
"#,
        )
        .expect("symbol language definition is written");
    }

    fn write_enum_language(&self) {
        fs::write(
            &self.0,
            r#"language Colors {
  version: "0.1.0"
  root: program
  construct program {
    syntax { default: "program {" }
    block: [paint]
  }
  construct paint {
    syntax { default: "paint {color}" }
    fields { color: enum [red, green, blue] required }
  }
}
"#,
        )
        .expect("enum language definition is written");
    }

    fn write_continuation_language(&self) {
        fs::write(
            &self.0,
            r#"language Functions {
  version: "0.1.0"
  root: program
  construct program {
    syntax { default: "program {" }
    block: [function]
  }
  construct function {
    syntax { default: "fn {name}\\({parameters}\\) -> {result}" }
    fields {
      name: identifier required
      parameters: identifier required
      result: identifier required
    }
  }
}
"#,
        )
        .expect("continuation language definition is written");
    }
}

impl Drop for TempLanguage {
    fn drop(&mut self) {
        if let Some(parent) = self.0.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }
}

#[test]
fn standalone_server_explains_that_rust_host_projects_need_host_injection() {
    let language = TempLanguage::new();
    let project_root = language.0.parent().expect("language has project root");
    let manifest = project_root.join("langmeld.json");
    fs::write(
        &manifest,
        r#"{"schema_version":2,"language_source":{"kind":"rust_host"}}"#,
    )
    .expect("Rust-host manifest is written");

    let output = Command::new(env!("CARGO_BIN_EXE_langmeld-lsp"))
        .arg("--project")
        .arg(&manifest)
        .stdin(Stdio::null())
        .output()
        .expect("LSP server starts");

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("load_project_with_base_language"));
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
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 1, "character": 0 }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": uri, "version": 2 },
                "contentChanges": [{ "text": "program {\nprint \"hello\"\n}\n" }]
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": uri, "version": 1 },
                "contentChanges": [{ "text": "obsolete" }]
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
    assert_eq!(responses.len(), 5);
    assert_eq!(responses[0]["id"], 1);
    assert_eq!(
        responses[0]["result"]["capabilities"]["textDocumentSync"]["change"],
        2
    );
    assert_eq!(
        responses[0]["result"]["capabilities"]["completionProvider"]["resolveProvider"],
        false
    );
    assert_eq!(responses[1]["method"], "textDocument/publishDiagnostics");
    assert!(
        !responses[1]["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(responses[1]["params"]["version"], 1);
    assert_eq!(responses[2]["id"], 3);
    assert!(
        responses[2]["result"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "print")
    );
    assert_eq!(responses[3]["method"], "textDocument/publishDiagnostics");
    assert_eq!(responses[3]["params"]["version"], 2);
    assert!(
        responses[3]["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty(),
        "updated diagnostics: {}",
        responses[3]["params"]["diagnostics"]
    );
    assert_eq!(responses[4]["id"], 2);
}

#[test]
fn child_process_provides_generic_hover_and_semantic_tokens() {
    let language = TempLanguage::new();
    let uri = format!(
        "file://{}",
        language.0.with_file_name("editor.demo").display()
    );
    let messages = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": uri,
                "languageId": "demo",
                "version": 1,
                "text": "program {\nprint \"😀hi\"\n}\n"
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 1, "character": 9 }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/semanticTokens/full",
            "params": { "textDocument": { "uri": uri } }
        }),
        json!({ "jsonrpc": "2.0", "id": 4, "method": "shutdown", "params": null }),
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
    let by_id = |id| {
        responses
            .iter()
            .find(|response| response["id"] == id)
            .expect("request response exists")
    };
    let capabilities = &by_id(1)["result"]["capabilities"];
    assert_eq!(capabilities["hoverProvider"], true);
    assert_eq!(capabilities["semanticTokensProvider"]["full"], true);
    assert_eq!(
        capabilities["semanticTokensProvider"]["legend"]["tokenModifiers"][0],
        "declaration"
    );
    let hover = by_id(2)["result"]["contents"]["value"]
        .as_str()
        .expect("hover markdown is returned");
    assert!(hover.contains("statement.message"));
    assert!(hover.contains("Declared: `text`"));
    assert!(hover.contains("Value: `\"😀hi\"`"));
    let tokens = by_id(3)["result"]["data"]
        .as_array()
        .expect("semantic token data is an array");
    assert!(tokens.len() >= 5 && tokens.len() % 5 == 0);
    assert!(
        tokens
            .chunks_exact(5)
            .any(|token| token[3] == 2 && token[2] == 6)
    );
    assert_eq!(by_id(4)["result"], Value::Null);
}

#[test]
fn child_process_completes_enum_members_from_the_captured_field_schema() {
    let language = TempLanguage::new();
    language.write_enum_language();
    let uri = format!(
        "file://{}",
        language.0.with_file_name("colors.demo").display()
    );
    let messages = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": uri,
                "languageId": "colors",
                "version": 1,
                "text": "program {\npaint green\n}\n"
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 1, "character": 7 }
            }
        }),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": null }),
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
    let initialization = responses
        .iter()
        .find(|response| response["id"] == 1)
        .unwrap();
    assert_eq!(
        initialization["result"]["capabilities"]["completionProvider"]["resolveProvider"],
        false
    );
    let completion = responses
        .iter()
        .find(|response| response["id"] == 2)
        .unwrap();
    assert_eq!(completion["result"].as_array().unwrap().len(), 1);
    assert_eq!(completion["result"][0]["label"], "green");
    assert_eq!(completion["result"][0]["kind"], 12);
    assert_eq!(
        completion["result"][0]["textEdit"]["range"]["start"],
        json!({ "line": 1, "character": 6 })
    );
}

#[test]
fn child_process_completes_partial_enum_values_and_child_construct_snippets() {
    let language = TempLanguage::new();
    language.write_enum_language();
    let enum_uri = format!(
        "file://{}",
        language.0.with_file_name("partial.demo").display()
    );
    let block_uri = format!(
        "file://{}",
        language.0.with_file_name("block.demo").display()
    );
    let messages = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": enum_uri,
                "languageId": "colors",
                "version": 1,
                "text": "program {\npaint }\n}\n"
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": block_uri,
                "languageId": "colors",
                "version": 1,
                "text": "program {\n\n}\n"
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": enum_uri },
                "position": { "line": 1, "character": 6 }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": block_uri },
                "position": { "line": 1, "character": 0 }
            }
        }),
        json!({ "jsonrpc": "2.0", "id": 4, "method": "shutdown", "params": null }),
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
    let enum_completion = responses
        .iter()
        .find(|response| response["id"] == 2)
        .unwrap();
    assert_eq!(enum_completion["result"][0]["label"], "red");
    assert_eq!(enum_completion["result"][1]["label"], "green");
    assert_eq!(enum_completion["result"][2]["label"], "blue");
    let child_completion = responses
        .iter()
        .find(|response| response["id"] == 3)
        .unwrap();
    assert_eq!(child_completion["result"][0]["label"], "paint");
    assert_eq!(
        child_completion["result"][0]["textEdit"]["newText"],
        "paint ${1:color}"
    );
    assert_eq!(child_completion["result"][0]["insertTextFormat"], 2);
}

#[test]
fn child_process_completes_incomplete_pattern_without_replacing_prior_fields() {
    let language = TempLanguage::new();
    language.write_continuation_language();
    let uri = format!(
        "file://{}",
        language.0.with_file_name("functions.demo").display()
    );
    let messages = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": uri,
                "languageId": "functions",
                "version": 1,
                "text": "program {\nfn main\n}\n"
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 1, "character": 7 }
            }
        }),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": null }),
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
        .expect("completion requests are written");
    let output = child.wait_with_output().expect("language server exits");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = read_frames(&output.stdout);
    let completion = responses
        .iter()
        .find(|response| response["id"] == 2)
        .expect("completion response exists");
    let item = completion["result"]
        .as_array()
        .expect("completion items are an array")
        .iter()
        .find(|item| item["label"] == "Continue syntax")
        .expect("parser continuation is offered");
    assert_eq!(
        item["textEdit"]["newText"],
        "(${1:parameters}) -> ${2:result}"
    );
    assert_eq!(item["insertTextFormat"], 2);
    assert_eq!(
        item["textEdit"]["range"]["start"],
        json!({ "line": 1, "character": 7 })
    );
    assert_eq!(
        item["textEdit"]["range"]["end"],
        json!({ "line": 1, "character": 7 })
    );
}

#[test]
fn child_process_publishes_nested_document_symbols_with_utf16_ranges() {
    let language = TempLanguage::new();
    language.write_symbol_language();
    let uri = format!(
        "file://{}",
        language.0.with_file_name("symbols.demo").display()
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
                    "languageId": "symbols",
                    "version": 1,
                    "text": "program main {\n  fn 𐐀 -> unit\n  use 𐐀\n}\n"
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/documentSymbol",
            "params": { "textDocument": { "uri": uri } }
        }),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": null }),
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
    assert_eq!(responses.len(), 4);
    assert_eq!(
        responses[0]["result"]["capabilities"]["documentSymbolProvider"],
        true
    );
    assert_eq!(responses[2]["id"], 2);
    let symbols = responses[2]["result"]
        .as_array()
        .expect("symbols are an array");
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0]["name"], "program");
    assert_eq!(symbols[0]["children"][0]["name"], "𐐀");
    assert_eq!(
        symbols[0]["children"][0]["selectionRange"]["start"],
        json!({ "line": 1, "character": 5 })
    );
    assert_eq!(
        symbols[0]["children"][0]["selectionRange"]["end"],
        json!({ "line": 1, "character": 7 })
    );
    assert_eq!(responses[3]["id"], 3);
}

#[test]
fn child_process_resolves_definitions_and_references_across_open_documents() {
    let language = TempLanguage::new();
    language.write_symbol_language();
    let declaration_uri = format!(
        "file://{}",
        language.0.with_file_name("declaration.demo").display()
    );
    let reference_uri = format!(
        "file://{}",
        language.0.with_file_name("reference.demo").display()
    );
    let other_uri = format!(
        "file://{}",
        language.0.with_file_name("other.demo").display()
    );
    let messages = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": declaration_uri,
                "languageId": "symbols",
                "version": 1,
                "text": "program main {\n  fn 𐐀 -> unit\n}\n"
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": other_uri,
                "languageId": "symbols",
                "version": 1,
                "text": "program other {\n  fn outside -> unit\n}\n"
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": reference_uri,
                "languageId": "symbols",
                "version": 1,
                "text": "program main {\n  use 𐐀\n  use out\n}\n"
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": reference_uri },
                "position": { "line": 1, "character": 6 }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/references",
            "params": {
                "textDocument": { "uri": declaration_uri },
                "position": { "line": 1, "character": 5 },
                "context": { "includeDeclaration": true }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": reference_uri },
                "position": { "line": 1, "character": 6 }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": declaration_uri },
                "position": { "line": 1, "character": 5 }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": reference_uri },
                "position": { "line": 1, "character": 8 }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": reference_uri },
                "position": { "line": 2, "character": 9 }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": reference_uri, "version": 2 },
                "contentChanges": [{ "text": "program main {\n  use missing\n}\n" }]
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": reference_uri },
                "position": { "line": 1, "character": 6 }
            }
        }),
        json!({ "jsonrpc": "2.0", "id": 5, "method": "shutdown", "params": null }),
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
    let by_id = |id| {
        responses
            .iter()
            .find(|response| response["id"] == id)
            .expect("request response exists")
    };
    assert_eq!(
        by_id(1)["result"]["capabilities"]["definitionProvider"],
        true
    );
    assert_eq!(
        by_id(1)["result"]["capabilities"]["referencesProvider"],
        true
    );
    let definition = &by_id(2)["result"][0];
    assert_eq!(definition["uri"], declaration_uri);
    assert_eq!(
        definition["range"]["start"],
        json!({ "line": 1, "character": 5 })
    );
    assert_eq!(
        definition["range"]["end"],
        json!({ "line": 1, "character": 7 })
    );
    let references = by_id(3)["result"]
        .as_array()
        .expect("references are an array");
    assert_eq!(references.len(), 2);
    assert_eq!(references[0]["uri"], declaration_uri);
    assert_eq!(references[1]["uri"], reference_uri);
    assert_eq!(
        references[1]["range"]["start"],
        json!({ "line": 1, "character": 6 })
    );
    assert!(
        by_id(6)["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|hover| hover.contains("Resolved to `𐐀`"))
    );
    assert!(
        by_id(7)["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|hover| hover.contains("Declared symbol"))
    );
    assert_eq!(by_id(8)["result"].as_array().unwrap().len(), 1);
    assert_eq!(by_id(8)["result"][0]["label"], "𐐀");
    assert_eq!(
        by_id(8)["result"][0]["textEdit"]["range"],
        json!({
            "start": { "line": 1, "character": 6 },
            "end": { "line": 1, "character": 8 }
        })
    );
    assert_eq!(by_id(9)["result"], json!([]));
    assert!(responses.iter().any(|response| {
        response["method"] == "textDocument/publishDiagnostics"
            && response["params"]["uri"] == reference_uri
            && response["params"]["version"] == 1
            && response["params"]["diagnostics"]
                .as_array()
                .is_some_and(|diagnostics| diagnostics.iter().any(|item| item["code"] == "LM2001"))
    }));
    assert!(responses.iter().any(|response| {
        response["method"] == "textDocument/publishDiagnostics"
            && response["params"]["uri"] == reference_uri
            && response["params"]["version"] == 2
            && response["params"]["diagnostics"]
                .as_array()
                .is_some_and(|diagnostics| diagnostics.iter().any(|item| item["code"] == "LM2001"))
    }));
    assert_eq!(by_id(4)["result"], json!([]));
    assert_eq!(by_id(5)["result"], Value::Null);
}
