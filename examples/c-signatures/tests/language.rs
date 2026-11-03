use csignatures_language::{
    RuntimeContext, analyze_sources, generate_rust, load_language, load_language_runtime,
    load_project, load_project_runtime, parse_source,
};
use langmeld::core::SourceFile;
use langmeld_lsp::{LanguageServerConfig, serve_configured};
use std::{
    fs,
    io::{BufRead, Cursor, Read, Write},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn custom_c_parser_lowers_prototypes_to_compilable_rust_ffi() {
    let language = load_language().expect("compiled language model loads");
    assert_eq!(language.spec().name, "CSignatures");
    let runtime_language = load_language_runtime().expect("runtime language model loads");
    assert_eq!(language.spec(), runtime_language.spec());
    let compiled_project = load_project().expect("compiled project model loads");
    let runtime_project = load_project_runtime().expect("runtime project model loads");
    assert_eq!(
        compiled_project.language.spec(),
        runtime_project.language.spec()
    );

    let source = SourceFile::new("math.c", include_str!("../examples/math.c"));
    let parsed = parse_source(&source).expect("custom parser prepares");
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(parsed.custom_parser_spans.len(), 1);
    let translation_unit = &parsed.program.nodes[0];
    assert_eq!(translation_unit.construct, "translation_unit");
    assert_eq!(translation_unit.children.len(), 2);
    assert_eq!(translation_unit.children[0].construct, "prototype0");
    assert_eq!(translation_unit.children[1].construct, "prototype2");

    let sources = vec![source];
    let programs = vec![parsed.program];
    let analysis = analyze_sources(&language, &sources, &[&programs[0]])
        .expect("generic analysis runs on custom syntax nodes");
    assert!(
        analysis.diagnostics.is_empty(),
        "{:#?}",
        analysis.diagnostics
    );
    let rust = generate_rust(&RuntimeContext {
        language: &language,
        sources: &sources,
        programs: &programs,
        analysis: &analysis,
    })
    .expect("host lowers C prototypes to Rust FFI declarations");
    assert_eq!(
        rust,
        "unsafe extern \"C\" {\n    pub fn reset();\n    pub fn add(left: i32, right: i32) -> i32;\n}\n"
    );

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "langmeld-c-signatures-output-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).expect("temporary compiler directory is created");
    let source_path = directory.join("generated.rs");
    fs::write(&source_path, rust).expect("generated Rust bindings are written");
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let compiled = Command::new(rustc)
        .args(["--edition=2024", "--crate-type=lib", "--emit=metadata"])
        .arg("--out-dir")
        .arg(&directory)
        .arg(&source_path)
        .output()
        .expect("Rust compiler starts");
    let _ = fs::remove_dir_all(&directory);
    assert!(
        compiled.status.success(),
        "generated Rust bindings did not compile:\n{}\n{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
}

#[test]
fn custom_c_parser_reports_unsupported_types() {
    let source = SourceFile::new("unsupported.c", "long count(void);\n");
    let parsed = parse_source(&source).expect("custom parser prepares");
    assert!(parsed.program.nodes.is_empty());
    assert_eq!(parsed.diagnostics.len(), 1);
    assert_eq!(parsed.diagnostics[0].code, "LMCS1001");
    assert!(
        parsed.diagnostics[0]
            .message
            .contains("unsupported C return type")
    );
}

#[test]
fn custom_c_parser_is_rejected_by_declarative_tree_sitter_export() {
    let language = load_language().expect("compiled language model loads");
    let error = langmeld::language::generate_tree_sitter(&language)
        .expect_err("a custom C parser cannot be represented by the declarative exporter");
    assert!(error.contains("custom parser `c_translation_unit`"));
    assert!(error.contains("construct `translation_unit`"));
}

#[test]
fn embedded_lsp_uses_the_host_registered_c_parser() {
    let language = load_language().expect("compiled language model loads");
    let parser = csignatures_language::parser_for(language)
        .expect("C parser callbacks are registered by the host");
    let requests = [
        serde_json::json!({"jsonrpc":"2.0", "id":1, "method":"initialize", "params":{}}),
        serde_json::json!({"jsonrpc":"2.0", "method":"initialized", "params":{}}),
        serde_json::json!({
            "jsonrpc":"2.0", "method":"textDocument/didOpen",
            "params":{"textDocument":{
                "uri":"file:///math.c", "languageId":"c", "version":1,
                "text":"int add(int left, int right);   \n"
            }}
        }),
        serde_json::json!({
            "jsonrpc":"2.0", "method":"textDocument/didOpen",
            "params":{"textDocument":{
                "uri":"file:///unsupported.c", "languageId":"c", "version":1,
                "text":"long count(void);\n"
            }}
        }),
        serde_json::json!({
            "jsonrpc":"2.0", "id":3, "method":"textDocument/formatting",
            "params":{"textDocument":{"uri":"file:///math.c", "version":1},
                "options":{"tabSize":4, "insertSpaces":true}}
        }),
        serde_json::json!({
            "jsonrpc":"2.0", "method":"textDocument/didOpen",
            "params":{"textDocument":{
                "uri":"file:///malformed.c", "languageId":"c", "version":1,
                "text":"int broken(void);\n"
            }}
        }),
        serde_json::json!({
            "jsonrpc":"2.0", "id":4, "method":"textDocument/formatting",
            "params":{"textDocument":{"uri":"file:///malformed.c", "version":1},
                "options":{"tabSize":4, "insertSpaces":true}}
        }),
        serde_json::json!({"jsonrpc":"2.0", "id":2, "method":"shutdown", "params":null}),
        serde_json::json!({"jsonrpc":"2.0", "method":"exit", "params":null}),
    ];
    let mut input = Vec::new();
    for request in requests {
        let body = serde_json::to_vec(&request).expect("request serializes");
        write!(input, "Content-Length: {}\r\n\r\n", body.len()).expect("header writes");
        input.extend_from_slice(&body);
    }
    let mut output = Vec::new();
    assert!(
        serve_configured(
            Cursor::new(input),
            &mut output,
            LanguageServerConfig::new(parser).with_formatting_provider(
                |_, source, _, _| {
                    if source.text.contains("broken") {
                        Ok("not a C declaration\n".to_owned())
                    } else {
                        Ok(source.text.trim().to_owned() + "\n")
                    }
                },
            ),
        )
        .expect("embedded LSP serves the custom parser"),
        "server observes shutdown before exit"
    );

    let messages = read_lsp_messages(output);
    let capabilities = messages
        .iter()
        .find(|message| message["id"] == 1)
        .expect("initialize response exists")
        .pointer("/result/capabilities")
        .expect("server capabilities are returned");
    assert_eq!(capabilities["documentFormattingProvider"], true);
    assert_eq!(
        capabilities["codeActionProvider"]["codeActionKinds"],
        serde_json::json!(["source"])
    );
    assert_eq!(
        messages
            .iter()
            .find(|message| message["id"] == 3)
            .expect("custom formatter response exists")["result"][0]["newText"],
        "int add(int left, int right);\n"
    );
    assert_eq!(
        messages
            .iter()
            .find(|message| message["id"] == 4)
            .expect("invalid custom formatter response exists")["result"],
        serde_json::json!([]),
        "invalid formatter output must not become an edit"
    );
    let valid = diagnostics_for(&messages, "file:///math.c");
    assert!(valid.is_empty(), "valid C declaration has no diagnostics");
    let invalid = diagnostics_for(&messages, "file:///unsupported.c");
    assert_eq!(invalid.len(), 1);
    assert_eq!(invalid[0]["code"], "LMCS1001");
}

fn diagnostics_for(messages: &[serde_json::Value], uri: &str) -> Vec<serde_json::Value> {
    messages
        .iter()
        .find(|message| {
            message["method"] == "textDocument/publishDiagnostics"
                && message
                    .pointer("/params/uri")
                    .and_then(serde_json::Value::as_str)
                    == Some(uri)
        })
        .and_then(|message| message.pointer("/params/diagnostics"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .expect("server publishes diagnostics for the opened document")
}

fn read_lsp_messages(output: Vec<u8>) -> Vec<serde_json::Value> {
    let mut reader = Cursor::new(output);
    let mut messages = Vec::new();
    loop {
        let mut content_length = None;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).expect("response header reads") == 0 {
                return messages;
            }
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length: ") {
                content_length = Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .expect("response length is numeric"),
                );
            }
        }
        let mut body = vec![0; content_length.expect("response has a length header")];
        reader.read_exact(&mut body).expect("response body reads");
        messages.push(serde_json::from_slice(&body).expect("response body is JSON"));
    }
}
