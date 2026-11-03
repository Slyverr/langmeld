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
  source_extension: ".demo"
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

    fn write_rename_language(&self) {
        fs::write(
            &self.0,
            r#"language RenameSymbols {
  version: "0.1.0"
  root: program
  construct program {
    syntax { default: "program {namespace} {" }
    fields { namespace: identifier required }
    block: [function, usage]
  }
  construct function {
    syntax { default: "fn {label} {name} -> {result}" }
    fields {
      label: text required
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
    syntax { default: "use {label} {target}" }
    fields {
      label: text required
      target: ref<function> required
    }
  }
}
"#,
        )
        .expect("rename language definition is written");
    }

    fn write_lexical_rename_language(&self) {
        fs::write(
            &self.0,
            r#"language LexicalRename {
  version: "0.1.0"
  root: program
  construct program {
    syntax { default: "program {" }
    block: [module]
  }
  construct module {
    syntax { default: "module {name} {" }
    fields { name: identifier required }
    block: [module, function, usage]
  }
  construct function {
    syntax { default: "fn {name}" }
    fields { name: identifier required }
    symbol {
      kind: function
      name: name
      scope: module
      scope_name: name
      scope_lookup: lexical_parents
    }
  }
  construct usage {
    syntax { default: "use {target}" }
    fields { target: ref<function> required }
  }
}
"#,
        )
        .expect("lexical rename language definition is written");
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

    fn as_project(&self) -> PathBuf {
        let root = self.0.parent().expect("language has project root");
        fs::create_dir_all(root.join("src")).expect("project source root is created");
        fs::write(
            root.join("src/main.demo"),
            "program {\nprint \"project source\"\n}\n",
        )
        .expect("project source is written");
        let manifest = root.join("langmeld.json");
        fs::write(
            &manifest,
            r#"{"schema_version":1,"language":"language.lm","source_roots":["src"]}"#,
        )
        .expect("project manifest is written");
        manifest
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

#[test]
fn definitions_mode_provides_lsp_support_for_modular_lm_sources() {
    let root_uri = "file:///language/language.lm";
    let module_uri = "file:///language/constructs/program.lm";
    let valid_root = "language Demo {\n  version: \"0.1.0\"\n  root: program\n}\n";
    let valid_module =
        "constructs {\n  construct program {\n    syntax { default: \"program {\" }\n  }\n}\n";
    let messages = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": root_uri,
                "languageId": "langmeld-definition",
                "version": 1,
                "text": valid_root
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": module_uri,
                "languageId": "langmeld-definition",
                "version": 1,
                "text": valid_module
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/semanticTokens/full",
            "params": { "textDocument": { "uri": module_uri } }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": root_uri },
                "position": { "line": 1, "character": 2 }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "textDocument/formatting",
            "params": {
                "textDocument": { "uri": module_uri },
                "options": { "tabSize": 2, "insertSpaces": true }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": root_uri, "version": 2 },
                "contentChanges": [{ "text": "language Demo {\n  version: \"0.1.0\"\n  root: \n}\n" }]
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
        .arg("--definitions")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("definition language server starts");
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
    let root_diagnostics = responses
        .iter()
        .find(|response| {
            response["method"] == "textDocument/publishDiagnostics"
                && response["params"]["uri"] == root_uri
                && response["params"]["version"] == 1
        })
        .expect("valid root definition diagnostics are published");
    assert!(
        root_diagnostics["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let module_diagnostics = responses
        .iter()
        .find(|response| {
            response["method"] == "textDocument/publishDiagnostics"
                && response["params"]["uri"] == module_uri
                && response["params"]["version"] == 1
        })
        .expect("valid modular definition diagnostics are published");
    assert!(
        module_diagnostics["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let semantic_tokens = responses
        .iter()
        .find(|response| response["id"] == 2)
        .expect("definition semantic tokens are returned");
    assert!(
        !semantic_tokens["result"]["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let completions = responses
        .iter()
        .find(|response| response["id"] == 3)
        .expect("definition completions are returned");
    assert!(!completions["result"].as_array().unwrap().is_empty());
    assert!(
        responses
            .iter()
            .find(|response| response["id"] == 4)
            .expect("definition formatting is returned")["result"]
            .as_array()
            .is_some()
    );
    let invalid_diagnostics = responses
        .iter()
        .find(|response| {
            response["method"] == "textDocument/publishDiagnostics"
                && response["params"]["uri"] == root_uri
                && response["params"]["version"] == 2
        })
        .expect("invalid definition diagnostics are published");
    assert!(
        !invalid_diagnostics["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(responses.iter().any(|response| response["id"] == 1));
    assert!(responses.iter().any(|response| response["id"] == 5));
}

#[test]
fn definition_project_mode_uses_the_explicit_root_and_reports_imported_file_errors() {
    let language = TempLanguage::new();
    let root = language.0.clone();
    fs::write(
        &root,
        "language Demo {\n  version: \"0.1.0\"\n  root: program\n}\n",
    )
    .expect("project root definition is written");
    let constructs = root.parent().unwrap().join("constructs");
    fs::create_dir_all(&constructs).expect("constructs directory is created");
    let module = constructs.join("program.lm");
    fs::write(
        &module,
        "constructs {\n  construct program {\n    syntax { default: \"program {\" }\n  }\n}\n",
    )
    .expect("construct module is written");
    let module_uri = format!("file://{}", module.display());
    let messages = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": module_uri,
                "languageId": "langmeld-definition",
                "version": 1,
                "text": "constructs {"
            }}
        }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": null }),
        json!({ "jsonrpc": "2.0", "method": "exit", "params": null }),
    ];
    let mut input = Vec::new();
    for message in &messages {
        input.extend(frame(message));
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_langmeld-lsp"))
        .arg("--definitions")
        .arg("--root")
        .arg(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("definition project language server starts");
    child
        .stdin
        .take()
        .expect("LSP input is piped")
        .write_all(&input)
        .expect("LSP requests are written");
    let output = child.wait_with_output().expect("LSP process exits");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = read_frames(&output.stdout);
    assert!(responses.iter().any(|response| {
        response["method"] == "textDocument/publishDiagnostics"
            && response["params"]["uri"] == module_uri
            && !response["params"]["diagnostics"]
                .as_array()
                .unwrap()
                .is_empty()
    }));
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
fn project_mode_registers_language_and_manifest_watchers() {
    let language = TempLanguage::new();
    let manifest = language.as_project();
    let messages = [
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "capabilities": { "workspace": {
                "didChangeWatchedFiles": { "dynamicRegistration": true }
            }}}
        }),
        json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": null }),
        json!({ "jsonrpc": "2.0", "method": "exit", "params": null }),
    ];
    let mut input = Vec::new();
    for message in &messages {
        input.extend(frame(message));
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_langmeld-lsp"))
        .arg("--project")
        .arg("langmeld.json")
        .current_dir(manifest.parent().expect("project has a root"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("project server starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(&input)
        .expect("protocol requests are written");
    let output = child.wait_with_output().expect("project server exits");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = read_frames(&output.stdout);
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0]["id"], 1);
    assert_eq!(responses[1]["method"], "client/registerCapability");
    let watchers = responses[1]["params"]["registrations"][0]["registerOptions"]["watchers"]
        .as_array()
        .expect("watchers are registered");
    assert!(
        watchers
            .iter()
            .any(|watcher| { watcher["globPattern"]["pattern"] == "**/*.lm" })
    );
    assert!(
        watchers
            .iter()
            .any(|watcher| { watcher["globPattern"]["pattern"] == "langmeld.json" })
    );
    assert!(watchers.iter().all(|watcher| {
        watcher["globPattern"]["baseUri"]
            .as_str()
            .is_some_and(|uri| uri.starts_with("file://"))
    }));
    assert_eq!(responses[2]["id"], 2);
}

#[test]
fn project_lsp_completes_constructs_from_a_declared_language_extension() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/declarative-interpreter/langmeld.json")
        .canonicalize()
        .expect("extension consumer manifest exists");
    let project_root = manifest.parent().expect("manifest has a project root");
    let source_uri = format!(
        "file://{}",
        project_root.join("examples/editor.ledger.lmprog").display()
    );
    let messages = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": source_uri,
                "languageId": "ledger",
                "version": 1,
                "text": "program {\n\n}\n"
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": source_uri },
                "position": { "line": 1, "character": 0 }
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
        .args(["--project"])
        .arg(&manifest)
        .current_dir(project_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("project language server starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(&input)
        .expect("LSP requests are written");
    let output = child.wait_with_output().expect("project server exits");
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
    let items = completion["result"].as_array().expect("items are returned");
    assert!(
        items.iter().any(|item| {
            item["label"] == "subtract"
                && item["textEdit"]["newText"]
                    .as_str()
                    .is_some_and(|text| text.starts_with("subtract "))
        }),
        "completion includes the extension-defined subtract construct: {items:#?}"
    );
}

#[test]
fn formatting_and_source_action_return_safe_versioned_edits() {
    let language = TempLanguage::new();
    let uri = format!(
        "file://{}",
        language.0.with_file_name("format.demo").display()
    );
    let messages = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": uri,
                "languageId": "demo",
                "version": 2,
                "text": "program {\nprint \"brace {value}\" # keep }\n}\n"
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/formatting",
            "params": {
                "textDocument": { "uri": uri, "version": 2 },
                "options": { "tabSize": 2, "insertSpaces": true }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/codeAction",
            "params": {
                "textDocument": { "uri": uri, "version": 2 },
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 1, "character": 0 }
                },
                "context": { "diagnostics": [], "only": ["source"] }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "textDocument/codeAction",
            "params": {
                "textDocument": { "uri": uri, "version": 2 },
                "range": {
                    "start": { "line": 9, "character": 0 },
                    "end": { "line": 10, "character": 0 }
                },
                "context": { "diagnostics": [] }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": "file:///broken.demo",
                "languageId": "demo",
                "version": 1,
                "text": "program {\nunknown\n}\n"
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "textDocument/formatting",
            "params": {
                "textDocument": { "uri": "file:///broken.demo", "version": 1 },
                "options": { "tabSize": 2, "insertSpaces": true }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": uri, "version": 3 },
                "contentChanges": [{ "text": "program {\n  print \"brace {value}\" # keep }\n}\n" }]
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "textDocument/formatting",
            "params": {
                "textDocument": { "uri": uri, "version": 2 },
                "options": { "tabSize": 2, "insertSpaces": true }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "textDocument/codeAction",
            "params": {
                "textDocument": { "uri": uri, "version": 2 },
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 1, "character": 0 }
                },
                "context": { "diagnostics": [] }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "textDocument/formatting",
            "params": {
                "textDocument": { "uri": uri, "version": 3 },
                "options": { "tabSize": 0, "insertSpaces": true }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": "file:///unclosed.demo",
                "languageId": "demo",
                "version": 1,
                "text": "program {\nprint \"open\""
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "textDocument/codeAction",
            "params": {
                "textDocument": { "uri": "file:///unclosed.demo", "version": 1 },
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 1, "character": 0 }
                },
                "context": {
                    "diagnostics": [{
                        "range": {
                            "start": { "line": 1, "character": 12 },
                            "end": { "line": 1, "character": 12 }
                        },
                        "severity": 1,
                        "code": "LM1204",
                        "source": "langmeld",
                        "message": "expected `}` before the end of the file"
                    }],
                    "only": ["quickfix"]
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": "file:///unclosed.demo", "version": 2 },
                "contentChanges": [{ "text": "program {\nprint \"open\"\n}\n" }]
            }
        }),
        json!({ "jsonrpc": "2.0", "id": 8, "method": "shutdown", "params": null }),
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
    let capabilities = &responses[0]["result"]["capabilities"];
    assert_eq!(capabilities["documentFormattingProvider"], true);
    let action_kinds = capabilities["codeActionProvider"]["codeActionKinds"]
        .as_array()
        .expect("code action kinds are advertised");
    assert!(action_kinds.contains(&json!("source")));
    assert!(action_kinds.contains(&json!("quickfix")));

    let formatting = responses
        .iter()
        .find(|response| response["id"] == 2)
        .expect("formatting response exists");
    assert_eq!(
        formatting["result"].as_array().unwrap().len(),
        1,
        "valid, unformatted input should produce a TextEdit: {responses:#?}"
    );
    assert_eq!(
        formatting["result"][0]["newText"],
        "program {\n  print \"brace {value}\" # keep }\n}\n"
    );

    let action = responses
        .iter()
        .find(|response| response["id"] == 3)
        .expect("formatting code action exists");
    assert_eq!(action["result"][0]["kind"], "source");
    assert_eq!(
        action["result"][0]["edit"]["documentChanges"][0]["textDocument"]["version"],
        2
    );
    assert_eq!(
        responses
            .iter()
            .find(|response| response["id"] == 4)
            .unwrap()["result"],
        json!([])
    );
    assert_eq!(
        responses
            .iter()
            .find(|response| response["id"] == 5)
            .unwrap()["result"],
        json!([])
    );
    assert_eq!(
        responses
            .iter()
            .find(|response| response["id"] == 6)
            .unwrap()["result"],
        json!([])
    );
    assert_eq!(
        responses
            .iter()
            .find(|response| response["id"] == 7)
            .unwrap()["result"],
        json!([])
    );
    assert_eq!(
        responses
            .iter()
            .find(|response| response["id"] == 9)
            .unwrap()["result"],
        json!([])
    );
    let quickfix = responses
        .iter()
        .find(|response| response["id"] == 10)
        .expect("missing-closer quick fix exists");
    assert_eq!(quickfix["result"][0]["kind"], "quickfix");
    assert_eq!(quickfix["result"][0]["title"], "Insert missing `}`");
    assert_eq!(
        quickfix["result"][0]["edit"]["documentChanges"][0]["textDocument"]["version"],
        1
    );
    assert_eq!(
        quickfix["result"][0]["edit"]["documentChanges"][0]["edits"][0]["newText"],
        "\n}"
    );
    let repaired_diagnostics = responses
        .iter()
        .find(|response| {
            response["method"] == "textDocument/publishDiagnostics"
                && response["params"]["uri"] == "file:///unclosed.demo"
                && response["params"]["version"] == 2
        })
        .expect("repaired document diagnostics are published");
    assert!(
        repaired_diagnostics["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
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

#[test]
fn project_navigation_includes_closed_files_and_returns_rename_edits_for_them() {
    let language = TempLanguage::new();
    language.write_symbol_language();
    let manifest = language.as_project();
    let root = manifest.parent().expect("project root exists");
    fs::write(
        &manifest,
        r#"{"schema_version":1,"language":"language.lm","source_roots":["src"],"source_extensions":["demo"]}"#,
    )
    .expect("project source extension is configured");
    let declaration_path = root.join("src/declaration.demo");
    let reference_path = root.join("src/reference.demo");
    fs::write(&declaration_path, "program main {\n  fn entry -> unit\n}\n")
        .expect("closed declaration source is written");
    fs::write(&reference_path, "program main {\n  use entry\n}\n")
        .expect("reference source is written");
    let declaration_uri = format!("file://{}", declaration_path.display());
    let reference_uri = format!("file://{}", reference_path.display());
    let messages = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": reference_uri,
                "languageId": "symbols",
                "version": 1,
                "text": "program main {\n  use entry\n}\n"
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": reference_uri },
                "position": { "line": 1, "character": 7 }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/references",
            "params": {
                "textDocument": { "uri": reference_uri },
                "position": { "line": 1, "character": 7 },
                "context": { "includeDeclaration": true }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": reference_uri },
                "position": { "line": 1, "character": 7 },
                "newName": "start"
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": declaration_uri,
                "languageId": "symbols",
                "version": 1,
                "text": "program main {\n  fn unsaved -> unit\n}\n"
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": reference_uri },
                "position": { "line": 1, "character": 7 }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didClose",
            "params": { "textDocument": { "uri": declaration_uri } }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": reference_uri },
                "position": { "line": 1, "character": 7 }
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
        .args(["--project"])
        .arg(&manifest)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("project language server starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(&input)
        .expect("LSP requests are written");
    let output = child.wait_with_output().expect("project server exits");
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
    assert_eq!(by_id(2)["result"][0]["uri"], declaration_uri);
    let references = by_id(3)["result"].as_array().expect("references returned");
    assert!(references.iter().any(|location| {
        location["uri"] == declaration_uri && location["range"]["start"]["line"] == 1
    }));
    assert!(
        references
            .iter()
            .any(|location| location["uri"] == reference_uri)
    );
    let changes = by_id(4)["result"]["changes"]
        .as_object()
        .expect("rename returns workspace changes");
    assert!(changes.contains_key(&declaration_uri));
    assert!(changes.contains_key(&reference_uri));
    assert_eq!(by_id(6)["result"], json!([]));
    assert_eq!(by_id(7)["result"][0]["uri"], declaration_uri);
}

#[test]
fn child_process_renames_resolved_symbols_with_safe_utf16_workspace_edits() {
    let language = TempLanguage::new();
    language.write_rename_language();
    let declaration_uri = format!(
        "file://{}",
        language.0.with_file_name("declarations.demo").display()
    );
    let reference_uri = format!(
        "file://{}",
        language.0.with_file_name("references.demo").display()
    );
    let messages = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": declaration_uri,
                "languageId": "rename-symbols",
                "version": 1,
                "text": "program Demo {\nfn \"😀\" api -> i32\nfn \"😀\" db -> i32\n}\n"
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": reference_uri,
                "languageId": "rename-symbols",
                "version": 1,
                "text": "program Demo {\nuse \"😀\" api\nuse \"😀\" missing\n}\n"
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": reference_uri },
                "position": { "line": 1, "character": 10 },
                "newName": "service"
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": reference_uri },
                "position": { "line": 1, "character": 10 },
                "newName": "db"
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": reference_uri },
                "position": { "line": 1, "character": 10 },
                "newName": "bad name"
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": reference_uri },
                "position": { "line": 2, "character": 10 },
                "newName": "service"
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": reference_uri },
                "position": { "line": 1, "character": 5 },
                "newName": "service"
            }
        }),
        json!({ "jsonrpc": "2.0", "id": 7, "method": "shutdown", "params": null }),
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
        .expect("rename requests are written");
    let output = child.wait_with_output().expect("language server exits");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = read_frames(&output.stdout);
    let capabilities = &responses
        .iter()
        .find(|response| response["id"] == 1)
        .expect("initialize response exists")["result"]["capabilities"];
    assert_eq!(capabilities["renameProvider"], true);

    let rename = responses
        .iter()
        .find(|response| response["id"] == 2)
        .expect("rename response exists");
    let declaration_edits = rename["result"]["changes"][declaration_uri]
        .as_array()
        .expect("declaration document edits are returned");
    assert_eq!(declaration_edits.len(), 1);
    assert_eq!(declaration_edits[0]["newText"], "service");
    assert_eq!(
        declaration_edits[0]["range"]["start"],
        json!({ "line": 1, "character": 8 })
    );
    assert_eq!(
        declaration_edits[0]["range"]["end"],
        json!({ "line": 1, "character": 11 })
    );
    let reference_edits = rename["result"]["changes"][reference_uri]
        .as_array()
        .expect("reference document edits are returned");
    assert_eq!(reference_edits.len(), 1);
    assert_eq!(reference_edits[0]["newText"], "service");
    assert_eq!(
        reference_edits[0]["range"]["start"],
        json!({ "line": 1, "character": 9 })
    );
    assert_eq!(
        reference_edits[0]["range"]["end"],
        json!({ "line": 1, "character": 12 })
    );
    for id in [3, 4, 5, 6] {
        assert_eq!(
            responses
                .iter()
                .find(|response| response["id"] == id)
                .expect("rename response exists")["result"],
            Value::Null,
            "rename request {id} must not return partial or unsafe edits"
        );
    }
}

#[test]
fn child_process_rename_uses_full_lexical_scope_paths() {
    let language = TempLanguage::new();
    language.write_lexical_rename_language();
    let uri = format!(
        "file://{}",
        language.0.with_file_name("scopes.demo").display()
    );
    let text = "program {\nmodule Shared {\nfn outer\nuse outer\nmodule Nest {\nfn foo\nuse foo\n}\nmodule Deep {\nfn busy\n}\n}\nmodule Other {\nmodule Nest {\nfn busy\n}\n}\n}\n";
    let messages = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": uri,
                "languageId": "lexical-rename",
                "version": 1,
                "text": text
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 5, "character": 4 },
                "newName": "busy"
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 2, "character": 4 },
                "newName": "busy"
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
        .expect("rename requests are written");
    let output = child.wait_with_output().expect("language server exits");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = read_frames(&output.stdout);
    let safe_rename = responses
        .iter()
        .find(|response| response["id"] == 2)
        .expect("rename response exists");
    let edits = safe_rename["result"]["changes"][uri]
        .as_array()
        .expect("same names in distinct lexical paths can be renamed");
    assert_eq!(edits.len(), 2);
    assert!(edits.iter().all(|edit| edit["newText"] == "busy"));
    assert_eq!(
        responses
            .iter()
            .find(|response| response["id"] == 3)
            .expect("rename response exists")["result"],
        Value::Null,
        "a descendant declaration must not capture references after rename"
    );
}
