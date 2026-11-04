//! Private server-state and protocol-helper tests. Public wire behavior is
//! exercised through the child-process tests in `tests/protocol.rs`.

use super::*;
use std::{fs, io::Cursor, time::SystemTime};

fn project_directory() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is after the Unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "langmeld lsp project reload-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("language/constructs"))
        .expect("project definition directory is created");
    fs::create_dir_all(root.join("src")).expect("project source directory is created");
    fs::write(
        root.join("langmeld.json"),
        r#"{"schema_version":1,"language":"language/language.langmeld","source_roots":["src"]}"#,
    )
    .expect("project manifest is written");
    fs::write(
            root.join("language/language.langmeld"),
            "language ReloadDemo {\n  version: \"0.1.0\"\n  root: program\n  source_extension: \".demo\"\n}\n",
        )
        .expect("language definition is written");
    write_project_constructs(&root, "print");
    fs::write(
        root.join("src/main.demo"),
        "program {\nprint \"project source\"\n}\n",
    )
    .expect("project source is written");
    root
}

fn write_project_constructs(root: &Path, keyword: &str) {
    fs::write(
            root.join("language/constructs/program.langmeld"),
            format!(
                "constructs {{\n  construct program {{\n    syntax {{ default: \"program {{\" }}\n    block: [statement]\n  }}\n  construct statement {{\n    syntax {{ default: \"{keyword} {{message}}\" }}\n    fields {{ message: text required }}\n  }}\n}}\n"
            ),
        )
        .expect("construct definition is written");
}

fn project_server(root: &Path) -> Server {
    let manifest = root.join("langmeld.json");
    let snapshot = load_project(&manifest).expect("project snapshot loads");
    let parser = configure_language(snapshot.language.clone()).expect("parser configures");
    LanguageServerConfig::new(parser)
        .with_project_snapshot(manifest, snapshot)
        .into_server()
        .expect("project server configures")
}

fn definition_project_server(root: &Path) -> Server {
    let root_path = root.join("language/language.langmeld");
    let language = langmeld_definition_language().expect("definition grammar validates");
    let parser = configure_language(language).expect("definition parser configures");
    LanguageServerConfig::new(parser)
        .with_definition_root(root_path)
        .into_server()
        .expect("definition project server configures")
}

fn add_definition_import(root: &Path, version: &str) -> PathBuf {
    let definitions = root.join("language");
    fs::create_dir_all(definitions.join("extensions"))
        .expect("definition extension directory is created");
    let root_path = definitions.join("language.langmeld");
    fs::write(
            &root_path,
            "language ReloadDemo {\n  version: \"0.1.0\"\n  root: program\n  source_extension: \".demo\"\n  import: \"extensions/extra.langmeld\"\n}\n",
        )
        .expect("root definition imports its extension");
    let extension_path = definitions.join("extensions/extra.langmeld");
    fs::write(&extension_path, extension_definition(version))
        .expect("extension definition is written");
    extension_path
}

fn extension_definition(version: &str) -> String {
    format!(
        "language ReloadDemo {{\n  version: \"{version}\"\n  root: program\n  construct note {{\n    syntax {{ default: \"note\" }}\n  }}\n}}\n"
    )
}

fn change_document(uri: &str, version: i64, text: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": {
            "textDocument": { "uri": uri, "version": version },
            "contentChanges": [{ "text": text }]
        }
    })
}

#[test]
fn project_navigation_index_discards_partial_results_when_a_source_uri_is_unavailable() {
    let root = project_directory();
    fs::write(
        root.join("src/second.demo"),
        "program {\nprint \"second source\"\n}\n",
    )
    .expect("second project source is written");
    let manifest = root.join("langmeld.json");
    let mut snapshot = load_project(&manifest).expect("project snapshot loads");
    assert_eq!(snapshot.sources.len(), 2);
    snapshot.sources[1].name = root.join("src/removed.demo").display().to_string();
    let parser = configure_language(snapshot.language.clone()).expect("parser configures");

    let (documents, limited) = index_project_sources(&parser, &snapshot);

    assert!(documents.is_empty());
    assert!(limited);
    fs::remove_dir_all(root).expect("temporary project is removed");
}

fn open_document(uri: &str, version: i64, text: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": { "textDocument": {
            "uri": uri,
            "languageId": "reload-demo",
            "version": version,
            "text": text
        }}
    })
}

fn watched_file(path: &Path) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "workspace/didChangeWatchedFiles",
        "params": { "changes": [{ "uri": path_to_file_uri(path).unwrap(), "type": 2 }] }
    })
}

#[test]
fn file_uri_round_trips_spaces_and_unicode_paths() {
    let root = project_directory();
    let path = root.join("language/constructs/café notes.langmeld");
    fs::write(&path, "constructs {}\n").expect("test definition file is written");
    let uri = path_to_file_uri(&path).expect("path converts to a file URI");
    let canonical_path = path.canonicalize().expect("test path canonicalizes");

    assert!(uri.contains("%20"));
    assert_eq!(
        file_uri_to_path(&uri).and_then(|path| path.canonicalize().ok()),
        Some(canonical_path)
    );
    assert!(file_uri_to_path("https://example.test/file.langmeld").is_none());
    assert!(file_uri_to_path("file:///tmp/bad%2Gpath.langmeld").is_none());

    fs::remove_dir_all(root).expect("temporary project is removed");
}

#[test]
fn definition_project_diagnostics_follow_open_imported_files_and_clear_after_edits() {
    let root = project_directory();
    let extension_path = add_definition_import(&root, "0.1.0");
    let extension_uri = path_to_file_uri(&extension_path).unwrap();
    let root_uri = path_to_file_uri(&root.join("language/language.langmeld")).unwrap();
    let mut server = definition_project_server(&root);

    let notifications = server.handle(&open_document(&extension_uri, 1, "language ReloadDemo {"));
    let imported_diagnostics = diagnostics_for(&notifications, &extension_uri)
        .expect("imported module diagnostics are published");
    assert!(!imported_diagnostics.is_empty());
    assert!(imported_diagnostics.iter().all(|diagnostic| {
        diagnostic.pointer("/range/start/line").is_some()
            && diagnostic.pointer("/range/end/line").is_some()
    }));

    let valid_extension = extension_definition("0.1.0");
    let notifications = server.handle(&change_document(&extension_uri, 2, &valid_extension));
    assert_eq!(
        diagnostics_for(&notifications, &extension_uri)
            .expect("import diagnostics are refreshed")
            .len(),
        0
    );

    let incompatible_extension = extension_definition("0.2.0");
    let notifications = server.handle(&change_document(&extension_uri, 3, &incompatible_extension));
    let root_diagnostics = diagnostics_for(&notifications, &root_uri)
        .expect("unlocated cross-module errors are published at the project root");
    assert!(root_diagnostics.iter().any(|diagnostic| {
        diagnostic["message"]
            .as_str()
            .is_some_and(|message| message.contains("different language name, version, or root"))
    }));
    assert_eq!(
        root_diagnostics[0].pointer("/range/start"),
        Some(&json!({ "line": 0, "character": 0 }))
    );
    assert_eq!(
        root_diagnostics[0].pointer("/range/end"),
        Some(&json!({ "line": 0, "character": 0 }))
    );

    let notifications = server.handle(&change_document(&extension_uri, 4, &valid_extension));
    assert_eq!(
        diagnostics_for(&notifications, &root_uri)
            .expect("stale root diagnostics are cleared")
            .len(),
        0
    );
    fs::remove_dir_all(root).expect("temporary project is removed");
}

#[test]
fn definition_project_watches_imports_and_revalidates_closed_files() {
    let root = project_directory();
    let extension_path = add_definition_import(&root, "0.1.0");
    let root_uri = path_to_file_uri(&root.join("language/language.langmeld")).unwrap();
    let mut server = definition_project_server(&root);
    server.handle(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "capabilities": { "workspace": {
            "didChangeWatchedFiles": { "dynamicRegistration": true }
        }}}
    }));
    let registrations = server.handle(&json!({
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {}
    }));
    let watchers = &registrations[0]["params"]["registrations"][0]["registerOptions"]["watchers"];
    assert!(watchers.as_array().unwrap().iter().any(|watcher| {
        watcher["globPattern"]["pattern"] == "**/*.langmeld"
            && watcher["globPattern"]["baseUri"]
                == path_to_file_uri(&root.join("language")).unwrap()
    }));

    fs::write(&extension_path, extension_definition("0.2.0"))
        .expect("incompatible extension is written");
    let notifications = server.handle(&watched_file(&extension_path));
    assert!(
        diagnostics_for(&notifications, &root_uri).is_some_and(|diagnostics| {
            diagnostics.iter().any(|diagnostic| {
                diagnostic["message"].as_str().is_some_and(|message| {
                    message.contains("different language name, version, or root")
                })
            })
        })
    );

    fs::write(&extension_path, extension_definition("0.1.0"))
        .expect("compatible extension is restored");
    let notifications = server.handle(&watched_file(&extension_path));
    assert_eq!(
        diagnostics_for(&notifications, &root_uri)
            .expect("watch event clears the project diagnostic")
            .len(),
        0
    );
    fs::remove_dir_all(root).expect("temporary project is removed");
}

#[test]
fn definition_project_rejects_import_cycles_root_escapes_and_oversized_modules() {
    let root = project_directory();
    let extension_path = add_definition_import(&root, "0.1.0");
    let extension_uri = path_to_file_uri(&extension_path).unwrap();
    let root_uri = path_to_file_uri(&root.join("language/language.langmeld")).unwrap();
    let mut server = definition_project_server(&root);

    let cycle = extension_definition("0.1.0").replace(
        "  construct note {",
        "  import: \"../language.langmeld\"\n  construct note {",
    );
    let notifications = server.handle(&open_document(&extension_uri, 1, &cycle));
    let root_diagnostics = diagnostics_for(&notifications, &root_uri)
        .expect("import-cycle diagnostic is published within the project");
    assert!(root_diagnostics.iter().any(|diagnostic| {
        diagnostic["message"]
            .as_str()
            .is_some_and(|message| message.contains("import cycle"))
    }));

    let outside_path = root.join("outside.langmeld");
    fs::write(&outside_path, "language Outside {}\n").expect("outside file is written");
    let escaped = extension_definition("0.1.0").replace(
        "  construct note {",
        "  import: \"../../outside.langmeld\"\n  construct note {",
    );
    let notifications = server.handle(&change_document(&extension_uri, 2, &escaped));
    let root_diagnostics = diagnostics_for(&notifications, &root_uri)
        .expect("root-escape diagnostic is kept within the project");
    assert!(root_diagnostics.iter().any(|diagnostic| {
        diagnostic["message"]
            .as_str()
            .is_some_and(|message| message.contains("escapes the language directory"))
    }));
    assert!(diagnostics_for(&notifications, &path_to_file_uri(&outside_path).unwrap()).is_none());

    let oversized = format!(
        "{}{}",
        extension_definition("0.1.0"),
        " ".repeat(DefinitionLimits::default().max_source_bytes)
    );
    server.handle(&json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didClose",
        "params": { "textDocument": { "uri": extension_uri } }
    }));
    fs::write(&extension_path, &oversized).expect("oversized module is written");
    let notifications = server.handle(&watched_file(&extension_path));
    let extension_diagnostics = diagnostics_for(&notifications, &extension_uri)
        .expect("resource-limit diagnostic is published for the imported file");
    assert!(
        extension_diagnostics.iter().any(|diagnostic| {
            diagnostic["message"]
                .as_str()
                .is_some_and(|message| message.contains("total parsing limit"))
        }),
        "unexpected diagnostics: {extension_diagnostics:?}"
    );

    fs::remove_dir_all(root).expect("temporary project is removed");
}

fn diagnostics_for<'a>(messages: &'a [Value], uri: &str) -> Option<&'a Vec<Value>> {
    messages
        .iter()
        .find(|message| {
            message["method"] == "textDocument/publishDiagnostics"
                && message.pointer("/params/uri").and_then(Value::as_str) == Some(uri)
        })
        .and_then(|message| message.pointer("/params/diagnostics"))
        .and_then(Value::as_array)
}

#[test]
fn watched_project_reload_updates_open_documents_and_keeps_last_valid_snapshot() {
    let root = project_directory();
    let construct_path = root.join("language/constructs/program.langmeld");
    let mut server = project_server(&root);

    let initialized = server.handle(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "capabilities": { "workspace": {
            "didChangeWatchedFiles": { "dynamicRegistration": true }
        }}}
    }));
    assert_eq!(
        initialized[0]["result"]["capabilities"]["textDocumentSync"]["change"],
        2
    );
    let registration = server.handle(&json!({
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {}
    }));
    assert_eq!(registration.len(), 1);
    assert_eq!(registration[0]["method"], "client/registerCapability");
    let watchers = &registration[0]["params"]["registrations"][0]["registerOptions"]["watchers"];
    assert!(
        watchers
            .as_array()
            .unwrap()
            .iter()
            .any(|watcher| { watcher["globPattern"]["pattern"] == "**/*.langmeld" })
    );
    assert!(
        watchers
            .as_array()
            .unwrap()
            .iter()
            .any(|watcher| { watcher["globPattern"]["pattern"] == "langmeld.json" })
    );
    assert!(
        watchers
            .as_array()
            .unwrap()
            .iter()
            .any(|watcher| { watcher["globPattern"]["pattern"] == "src/**" })
    );
    assert_eq!(
        watchers[0]["globPattern"]["baseUri"],
        path_to_file_uri(&root).unwrap()
    );
    assert!(
        watchers[0]["globPattern"]["baseUri"]
            .as_str()
            .unwrap()
            .contains("%20")
    );

    let old_uri = "file:///reload-old.demo";
    let old_open = server.handle(&open_document(
        old_uri,
        1,
        "program {\nprint \"old syntax\"\n}\n",
    ));
    assert_eq!(diagnostics_for(&old_open, old_uri).unwrap().len(), 0);

    write_project_constructs(&root, "say");
    let reloaded = server.handle(&watched_file(&construct_path));
    let old_diagnostics = diagnostics_for(&reloaded, old_uri).unwrap();
    assert!(old_diagnostics.iter().any(|item| item["severity"] == 1));
    let new_uri = "file:///reload-new.demo";
    let new_open = server.handle(&open_document(
        new_uri,
        1,
        "program {\nsay \"new syntax\"\n}\n",
    ));
    assert_eq!(diagnostics_for(&new_open, new_uri).unwrap().len(), 0);

    let project_source = root.join("src/main.demo");
    fs::write(&project_source, "program {\nsay \"updated source\"\n}\n")
        .expect("project source edit is written");
    server.handle(&watched_file(&project_source));
    assert_eq!(
        server.project.as_ref().unwrap().snapshot.sources[0].text,
        "program {\nsay \"updated source\"\n}\n"
    );
    let project_source_uri = path_to_file_uri(&project_source).unwrap();
    assert!(
        server
            .project
            .as_ref()
            .unwrap()
            .source_documents
            .get(&project_source_uri)
            .unwrap()
            .parsed
            .diagnostics
            .is_empty()
    );

    fs::create_dir_all(root.join("programs")).expect("replacement source root is created");
    fs::write(
        root.join("programs/next.demo"),
        "program {\nsay \"new root\"\n}\n",
    )
    .expect("replacement source is written");
    fs::write(
            root.join("langmeld.json"),
            r#"{"schema_version":1,"language":"language/language.langmeld","source_roots":["programs"]}"#,
        )
        .expect("updated project roots are written");
    let root_change = server.handle(&watched_file(&root.join("langmeld.json")));
    assert!(root_change.iter().any(|notification| {
        notification["method"] == "client/unregisterCapability"
            && notification["id"] == PROJECT_WATCHER_REFRESH_ID
    }));
    let registration = server.handle(&json!({
        "jsonrpc": "2.0",
        "id": PROJECT_WATCHER_REFRESH_ID,
        "result": null
    }));
    let watchers = &registration[0]["params"]["registrations"][0]["registerOptions"]["watchers"];
    assert!(
        watchers
            .as_array()
            .unwrap()
            .iter()
            .any(|watcher| { watcher["globPattern"]["pattern"] == "programs/**" })
    );
    assert!(
        !watchers
            .as_array()
            .unwrap()
            .iter()
            .any(|watcher| { watcher["globPattern"]["pattern"] == "src/**" })
    );

    fs::write(&construct_path, "constructs {\n").expect("invalid edit is written");
    let failed = server.handle(&watched_file(&construct_path));
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0]["method"], "window/logMessage");
    assert_eq!(failed[0]["params"]["type"], 2);
    assert!(
        failed[0]["params"]["message"]
            .as_str()
            .unwrap()
            .contains("keeping the last valid language")
    );

    let retained_uri = "file:///reload-retained.demo";
    let retained = server.handle(&open_document(
        retained_uri,
        1,
        "program {\nsay \"still valid\"\n}\n",
    ));
    assert_eq!(diagnostics_for(&retained, retained_uri).unwrap().len(), 0);
    assert!(old_diagnostics.iter().any(|item| item["severity"] == 1));

    fs::remove_dir_all(root).expect("temporary project is removed");
}

#[test]
fn lsp_positions_count_utf16_code_units() {
    assert_eq!(
        lsp_position("😀x\nπ", 4),
        json!({ "line": 0, "character": 2 })
    );
    assert_eq!(
        lsp_position("😀x\nπ", 7),
        json!({ "line": 1, "character": 0 })
    );
}

#[test]
fn content_changes_apply_utf16_ranges_sequentially() {
    let updated = apply_content_changes(
        "a😀b\r\nnext",
        &[json!({
            "range": {
                "start": { "line": 0, "character": 1 },
                "end": { "line": 0, "character": 3 }
            },
            "rangeLength": 2,
            "text": "x"
        })],
    );
    assert_eq!(updated.as_deref(), Some("axb\r\nnext"));
}

#[test]
fn content_changes_reject_invalid_ranges_without_partial_output() {
    let invalid = apply_content_changes(
        "a😀b",
        &[json!({
            "range": {
                "start": { "line": 0, "character": 2 },
                "end": { "line": 0, "character": 3 }
            },
            "text": "x"
        })],
    );
    assert_eq!(invalid, None);

    let invalid_length = apply_content_changes(
        "abc",
        &[json!({
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 2 }
            },
            "rangeLength": 1,
            "text": "x"
        })],
    );
    assert_eq!(invalid_length, None);
}

#[test]
fn content_changes_replace_full_document_and_enforce_size_limit() {
    let updated = apply_content_changes("old", &[json!({ "text": "new" })]);
    assert_eq!(updated.as_deref(), Some("new"));
    let too_large = "x".repeat(MAX_DOCUMENT_BYTES + 1);
    assert_eq!(
        apply_content_changes("old", &[json!({ "text": too_large })]),
        None
    );
    assert_eq!(offset_at_position("a\n", 1, 0), Some(2));
    assert_eq!(offset_at_position("a", 1, 0), None);
}

#[test]
fn keyword_completion_filters_prefix_and_uses_utf16_edit_ranges() {
    let text = "😀 λ";
    let items = keyword_completion_items(text, text.len(), ["if", "λambda", "return"].into_iter());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["label"], "λambda");
    assert_eq!(
        items[0]["textEdit"]["range"]["start"],
        json!({ "line": 0, "character": 3 })
    );
    assert_eq!(
        items[0]["textEdit"]["range"]["end"],
        json!({ "line": 0, "character": 4 })
    );
}

#[test]
fn keyword_completion_is_empty_without_keywords_and_initialization_is_gated() {
    let items = keyword_completion_items("", 0, std::iter::empty());
    assert!(items.is_empty());
    let response = initialize_response(json!(1), false, false, false, false, false, false);
    assert!(
        response["result"]["capabilities"]
            .get("completionProvider")
            .is_none()
    );
    assert!(
        response["result"]["capabilities"]
            .get("definitionProvider")
            .is_none()
    );
    assert!(
        response["result"]["capabilities"]
            .get("documentFormattingProvider")
            .is_none()
    );
    assert!(
        response["result"]["capabilities"]
            .get("referencesProvider")
            .is_none()
    );
    assert!(
        response["result"]["capabilities"]
            .get("renameProvider")
            .is_none()
    );
}

#[test]
fn construct_pattern_snippets_expand_fields_and_skip_pattern_operators() {
    assert_eq!(
        construct_pattern_snippet(r#"paint {color} at \({x}, {y}\)"#),
        Some(r#"paint ${1:color} at (${2:x}, ${3:y})"#.to_owned())
    );
    assert_eq!(construct_pattern_snippet("if [{condition}]"), None);
    assert_eq!(
        construct_pattern_snippet("block {"),
        Some("block {".to_owned())
    );
}

#[test]
fn parser_continuation_snippets_render_fields_literals_and_spacing() {
    let snippet = completion_continuation_snippet(&[
        CompletionPart::Literal("fn".to_owned()),
        CompletionPart::Capture("name".to_owned()),
        CompletionPart::Literal("(".to_owned()),
        CompletionPart::Capture("parameters".to_owned()),
        CompletionPart::Literal(")".to_owned()),
        CompletionPart::Literal("->".to_owned()),
        CompletionPart::Capture("result".to_owned()),
    ]);
    assert_eq!(
        snippet,
        Some("fn ${1:name}(${2:parameters}) -> ${3:result}".to_owned())
    );
}

#[test]
fn parser_continuation_snippets_escape_snippet_metacharacters_and_bound_size() {
    assert_eq!(
        completion_continuation_snippet(&[
            CompletionPart::Capture("field$name".to_owned()),
            CompletionPart::Literal("}".to_owned()),
        ]),
        Some(r"${1:field\$name}\}".to_owned())
    );
    assert_eq!(
        completion_continuation_snippet(&[CompletionPart::Literal("x".repeat(4097))]),
        None
    );
}

#[test]
fn embedded_server_runs_registered_custom_parser_and_analysis_callbacks() {
    use langmeld_core::{
        BehaviorStage, ConstructBuilder, Diagnostic, FieldKind, FieldSpec, LanguageBuilder, Span,
        SymbolSpec,
    };
    use langmeld_language::{
        CapturedValue, ConfiguredSemanticHookRegistration, CustomConstructMatch,
        CustomConstructParser, SemanticHookRegistration, Value as LangValue, WorkspaceDiagnostic,
        WorkspaceSymbolResolverRegistration, WorkspaceTypeAnalysisRegistration,
    };

    fn plain_semantic_hook(_: &Program, _: &langmeld_core::SourceFile, _: &mut Vec<Diagnostic>) {}

    let language = LanguageBuilder::new("Embedded")
        .version("0.1.0")
        .root("program")
        .behavior("semantic-check", BehaviorStage::SemanticAnalysis)
        .behavior("semantic-note", BehaviorStage::SemanticAnalysis)
        .behavior("resolve", BehaviorStage::SymbolResolution)
        .behavior("workspace-types", BehaviorStage::WorkspaceTypeAnalysis)
        .construct(
            ConstructBuilder::new("program")
                .syntax("program {")
                .block(["native"]),
        )
        .construct(
            ConstructBuilder::new("native")
                .syntax_variant("native", "host syntax {name}")
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .symbol(SymbolSpec::new("native", "name"))
                .custom_parser("parse-native"),
        )
        .build()
        .expect("host language definition validates");
    let parser = PreparedLanguage::new(language.clone())
        .expect("syntax patterns compile")
        .with_custom_parsers([CustomConstructParser::new(
            "parse-native",
            |source, offset| {
                let Some(rest) = source.text.get(offset..) else {
                    return Ok(None);
                };
                let line_end = rest.find('\n').unwrap_or(rest.len());
                let line = rest[..line_end].trim_end_matches('\r');
                let Some(name) = line.strip_prefix("native ") else {
                    return Ok(None);
                };
                let name_start = offset + "native ".len();
                let end = offset + line.len();
                Ok(Some(
                    CustomConstructMatch::new(Span::new(offset, end), "native").with_field(
                        "name",
                        CapturedValue::new(
                            LangValue::Identifier(name.to_owned()),
                            Span::new(name_start, end),
                        ),
                    ),
                ))
            },
        )])
        .expect("custom parser registration matches the language");
    assert!(configure_language(language).is_err());

    let missing_hook = serve_configured(
        Cursor::new(Vec::<u8>::new()),
        Vec::<u8>::new(),
        LanguageServerConfig::new(parser.clone()),
    )
    .expect_err("missing declared behavior is rejected before input is read");
    assert!(missing_hook.contains("semantic-check"));
    let duplicate_hook = serve_configured(
        Cursor::new(Vec::<u8>::new()),
        Vec::<u8>::new(),
        LanguageServerConfig::new(parser.clone())
            .with_semantic_hooks([SemanticHookRegistration::new(
                "semantic-note",
                plain_semantic_hook,
            )])
            .with_configured_semantic_hooks([ConfiguredSemanticHookRegistration::new(
                "semantic-note",
                |_, _, _| {},
            )]),
    )
    .expect_err("duplicate behavior IDs are rejected");
    assert!(duplicate_hook.contains("registered more than once"));
    let undeclared_hook = serve_configured(
        Cursor::new(Vec::<u8>::new()),
        Vec::<u8>::new(),
        LanguageServerConfig::new(parser.clone()).with_semantic_hooks([
            SemanticHookRegistration::new("semantic-check", plain_semantic_hook),
            SemanticHookRegistration::new("semantic-note", plain_semantic_hook),
            SemanticHookRegistration::new("extra", plain_semantic_hook),
        ]),
    )
    .expect_err("undeclared behavior IDs are rejected");
    assert!(undeclared_hook.contains("does not declare"));

    let config = LanguageServerConfig::new(parser)
        .with_semantic_hooks([SemanticHookRegistration::new(
            "semantic-note",
            plain_semantic_hook,
        )])
        .with_symbol_resolvers([WorkspaceSymbolResolverRegistration::new(
            "resolve",
            Box::new(|_, _, analysis| {
                analysis.diagnostics.push(WorkspaceDiagnostic {
                    source_index: 0,
                    source_id: None,
                    source_revision: None,
                    diagnostic: Diagnostic::error(
                        "HOST0002",
                        "host symbol resolver ran",
                        Span::new(10, 15),
                    ),
                });
            }),
        )])
        .with_type_analysis_hooks([WorkspaceTypeAnalysisRegistration::new(
            "workspace-types",
            Box::new(|_, _, _, _, _, diagnostics| {
                diagnostics.push(WorkspaceDiagnostic {
                    source_index: 0,
                    source_id: None,
                    source_revision: None,
                    diagnostic: Diagnostic::error(
                        "HOST0003",
                        "host type-analysis hook ran",
                        Span::new(10, 15),
                    ),
                });
            }),
        )])
        .with_configured_semantic_hooks([ConfiguredSemanticHookRegistration::new(
            "semantic-check",
            |program, source, diagnostics| {
                let has_native_node = program
                    .nodes
                    .first()
                    .and_then(|root| root.children.first())
                    .is_some_and(|node| node.construct == "native");
                if has_native_node {
                    diagnostics.push(Diagnostic::error(
                        "HOST0001",
                        "host semantic callback ran",
                        Span::new(10, source.text.len().saturating_sub(2)),
                    ));
                }
            },
        )])
        .with_completion_providers([CompletionProviderRegistration::new(|context| {
            let mut candidate = CompletionCandidate::new("host-value");
            candidate.kind = CompletionItemKind::Constant;
            candidate.detail = Some(format!(
                "{}:{}:{}",
                context.uri,
                context.workspace.len(),
                context.prefix
            ));
            let duplicate = CompletionCandidate::new("host-value");
            let oversized = CompletionCandidate::new("x".repeat(MAX_COMPLETION_LABEL_BYTES + 1));
            let mut candidates = vec![candidate, duplicate, oversized];
            candidates.extend(
                (0..MAX_COMPLETION_ITEMS + 10)
                    .map(|index| CompletionCandidate::new(format!("extra-{index}"))),
            );
            candidates
        })]);
    let requests = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": "file:///embedded.demo",
                "languageId": "embedded",
                "version": 1,
                "text": "program {\nnative node\n}\n"
            }}
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "textDocument/completion",
            "params": {
                "textDocument": { "uri": "file:///embedded.demo" },
                "position": { "line": 1, "character": 11 }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": "file:///embedded.demo" },
                "position": { "line": 1, "character": 11 },
                "newName": "renamed"
            }
        }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": null }),
        json!({ "jsonrpc": "2.0", "method": "exit", "params": null }),
    ];
    let mut input = Vec::new();
    for request in &requests {
        write_message(&mut input, request).expect("request framing succeeds");
    }
    let mut output = Vec::new();
    assert!(
        serve_configured(Cursor::new(input), &mut output, config)
            .expect("embedded server handles protocol requests")
    );
    let mut reader = Cursor::new(output);
    let mut responses = Vec::new();
    while let Some(message) = read_message(&mut reader).expect("response framing is valid") {
        responses.push(serde_json::from_slice::<Value>(&message).expect("response is JSON"));
    }
    let diagnostics = responses
        .iter()
        .find(|response| response["method"] == "textDocument/publishDiagnostics")
        .and_then(|response| response.pointer("/params/diagnostics"))
        .and_then(Value::as_array)
        .expect("host diagnostics are published");
    let completion = responses
        .iter()
        .find(|response| response["id"] == 4)
        .expect("host completion response exists");
    assert_eq!(completion["result"][0]["label"], "host-value");
    assert_eq!(
        completion["result"].as_array().unwrap().len(),
        MAX_COMPLETION_ITEMS
    );
    assert_eq!(
        completion["result"][0]["detail"],
        "file:///embedded.demo:1:node"
    );
    assert_eq!(
        completion["result"][0]["textEdit"]["range"]["start"],
        json!({ "line": 1, "character": 7 })
    );
    assert_eq!(
        responses
            .iter()
            .find(|response| response["id"] == 1)
            .unwrap()["result"]["capabilities"]["completionProvider"]["resolveProvider"],
        false
    );
    let initialize = responses
        .iter()
        .find(|response| response["id"] == 1)
        .expect("initialize response exists");
    assert!(
        initialize["result"]["capabilities"]
            .get("renameProvider")
            .is_none(),
        "host resolver callbacks do not advertise generic rename"
    );
    assert_eq!(
        responses
            .iter()
            .find(|response| response["id"] == 5)
            .expect("rename response exists")["result"],
        Value::Null
    );
    for code in ["HOST0001", "HOST0002", "HOST0003"] {
        assert!(
            diagnostics.iter().any(|item| item["code"] == code),
            "{code}"
        );
    }
}

#[test]
fn framing_reads_a_message_and_detects_oversized_bodies() {
    let body = br#"{"jsonrpc":"2.0","method":"initialized"}"#;
    let framed = format!(
        "Content-Length: {}\r\n\r\n{}",
        body.len(),
        String::from_utf8_lossy(body)
    );
    let mut reader = Cursor::new(framed.into_bytes());
    assert_eq!(read_message(&mut reader).unwrap().unwrap(), body);

    let mut oversized = Cursor::new(
        format!(
            "Content-Length: {}\r\n\r\n",
            protocol::MAX_MESSAGE_BYTES + 1
        )
        .into_bytes(),
    );
    assert!(read_message(&mut oversized).is_err());
}

#[test]
fn framing_rejects_duplicate_content_length_headers() {
    let mut reader = Cursor::new(b"Content-Length: 0\r\ncontent-length: 0\r\n\r\n".to_vec());
    assert!(read_message(&mut reader).is_err());
}
