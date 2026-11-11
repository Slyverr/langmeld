use super::*;
use serde_json::json;

#[test]
fn project_definitions_lsp_uses_the_open_buffer_for_import_validation() {
    let parent = TestDirectory::new();
    let project = parent.path().join("project with spaces");
    write_widget_project(&project);
    let module = project.join("language/constructs/statements.langmeld");
    let root = project.join("language/language.langmeld");
    let module_uri = url::Url::from_file_path(&module).unwrap().to_string();
    let root_uri = url::Url::from_file_path(&root).unwrap().to_string();
    let (mut child, mut stdin, receiver) = start_lsp(&["lsp", "--definitions"], Some(&project));

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    assert_eq!(receive_lsp_message(&receiver)["id"], 1);
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{
                    "uri":module_uri,
                    "languageId":"langmeld",
                    "version":1,
                    "text":"construct \"program\" { syntax { literal \"program\"; } }"
                }
            }
        }),
    );
    let diagnostics = receive_lsp_message(&receiver);
    assert_eq!(diagnostics["method"], "textDocument/publishDiagnostics");
    assert_eq!(
        diagnostics["params"]["diagnostics"][0]["code"],
        "LM-DEF-004"
    );

    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{
                    "uri":root_uri,
                    "languageId":"langmeld",
                    "version":1,
                    "text":"language \"Widget\" { import \"constructs/statements.langmeld\"; construct \"program\" { syntax { literal \"program\"; } block \"{\" \"}\" { allow \"missing\"; } } }"
                }
            }
        }),
    );
    let diagnostics = [
        receive_lsp_message(&receiver),
        receive_lsp_message(&receiver),
    ];
    assert!(
        diagnostics
            .iter()
            .all(|message| { message["method"] == "textDocument/publishDiagnostics" })
    );
    assert!(diagnostics.iter().any(|message| {
        message["params"]["diagnostics"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item["code"] == "LM-DEF-004"))
    }));

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
fn project_definitions_lsp_completes_references_and_import_paths() {
    let parent = TestDirectory::new();
    let project = parent.path().join("project");
    write_widget_project(&project);
    let root = project.join("language/language.langmeld");
    let module = project.join("language/constructs/statements.langmeld");
    let second_module = project.join("language/constructs/alpha.langmeld");
    let root_source = "language \"Widget\" { import \"constructs/statements.langmeld\"; import \"constructs/alpha.langmeld\"; construct \"program\" { syntax { literal \"program\"; } block \"{\" \"}\" { allow \"print\"; } } }";
    let module_source = "construct \"print\" { implements \"statement\"; syntax { literal \"print\"; identifier \"name\"; literal \";\"; } }";
    let second_module_source = "construct \"zebra\" { syntax { literal \"zebra\"; } }";
    fs::write(
        parent.path().join("outside.langmeld"),
        "construct \"private\" {}",
    )
    .unwrap();
    fs::write(&root, root_source).unwrap();
    fs::write(&module, module_source).unwrap();
    fs::write(&second_module, second_module_source).unwrap();
    let root_uri = url::Url::from_file_path(fs::canonicalize(&root).unwrap())
        .unwrap()
        .to_string();
    let module_uri = url::Url::from_file_path(fs::canonicalize(&module).unwrap())
        .unwrap()
        .to_string();
    let (mut child, mut stdin, receiver) = start_lsp(&["lsp", "--definitions"], Some(&project));

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    assert_eq!(receive_lsp_message(&receiver)["id"], 1);
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    for (uri, text) in [(&root_uri, root_source), (&module_uri, module_source)] {
        send_lsp_message(
            &mut stdin,
            json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"langmeld","version":1,"text":text}}}),
        );
        let count = if uri == &root_uri { 1 } else { 2 };
        for _ in 0..count {
            assert_eq!(
                receive_lsp_message(&receiver)["method"],
                "textDocument/publishDiagnostics"
            );
        }
    }

    let allow_offset = root_source.find("allow \"").unwrap() + "allow \"".len();
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":8,"method":"textDocument/completion","params":{"textDocument":{"uri":root_uri},"position":{"line":0,"character":allow_offset}}}),
    );
    let construct_completion = receive_lsp_message(&receiver);
    let construct_labels = construct_completion["result"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["label"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(construct_labels, ["program", "print", "zebra"]);

    for (id, uri, text, prefix, label, detail, kind) in [
        (2, &root_uri, root_source, "pri", "print", "Construct", 18),
        (
            3,
            &module_uri,
            module_source,
            "sta",
            "statement",
            "Category",
            18,
        ),
        (
            4,
            &root_uri,
            root_source,
            "constructs/stat",
            "constructs/statements.langmeld",
            ".langmeld definition",
            17,
        ),
        (
            5,
            &root_uri,
            root_source,
            "con",
            "constructs/",
            "Folder",
            19,
        ),
    ] {
        let offset = text.find(prefix).unwrap() + prefix.len();
        let position = json!({"line": 0, "character": offset});
        send_lsp_message(
            &mut stdin,
            json!({"jsonrpc":"2.0","id":id,"method":"textDocument/completion","params":{"textDocument":{"uri":uri},"position":position}}),
        );
        let completion = receive_lsp_message(&receiver);
        assert_eq!(completion["result"].as_array().unwrap().len(), 1);
        assert_eq!(completion["result"][0]["label"], label);
        assert_eq!(completion["result"][0]["detail"], detail);
        assert_eq!(completion["result"][0]["kind"], kind);
        assert_eq!(
            completion["result"][0]["textEdit"]["newText"],
            format!("\"{label}\"")
        );
    }

    let outside_import =
        root_source.replace("constructs/statements.langmeld", "../../outside.langmeld");
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":root_uri,"version":2},"contentChanges":[{"text":outside_import}]}}),
    );
    for _ in 0..2 {
        assert_eq!(
            receive_lsp_message(&receiver)["method"],
            "textDocument/publishDiagnostics"
        );
    }
    let outside_prefix = "../../outside";
    let outside_offset = outside_import.find(outside_prefix).unwrap() + outside_prefix.len();
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":6,"method":"textDocument/completion","params":{"textDocument":{"uri":root_uri},"position":{"line":0,"character":outside_offset}}}),
    );
    assert!(
        receive_lsp_message(&receiver)["result"]
            .as_array()
            .unwrap()
            .is_empty(),
        "path suggestions must not expose files outside the project root"
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":7,"method":"shutdown","params":null}),
    );
    assert_eq!(receive_lsp_message(&receiver)["id"], 7);
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"exit","params":null}),
    );
    drop(stdin);
    assert!(child.0.wait().unwrap().success());
}

#[test]
fn project_definitions_lsp_refreshes_references_from_disk_and_incomplete_buffers() {
    let parent = TestDirectory::new();
    let project = parent.path().join("project");
    write_widget_project(&project);
    let root = project.join("language/language.langmeld");
    let module = project.join("language/constructs/statements.langmeld");
    let root_source = fs::read_to_string(&root).unwrap();
    let module_uri = url::Url::from_file_path(&module).unwrap().to_string();
    let root_uri = url::Url::from_file_path(&root).unwrap().to_string();
    let (mut child, mut stdin, receiver) = start_lsp(&["lsp", "--definitions"], Some(&project));

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{"workspace":{"didChangeWatchedFiles":{"dynamicRegistration":true,"relativePatternSupport":true}}}}}),
    );
    let initialized = receive_lsp_message(&receiver);
    assert_eq!(initialized["id"], 1);
    assert_eq!(
        initialized["result"]["capabilities"]["documentSymbolProvider"],
        true
    );
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    let registration = receive_lsp_message(&receiver);
    assert_eq!(registration["method"], "client/registerCapability");
    let watcher = &registration["params"]["registrations"][0]["registerOptions"]["watchers"][0];
    assert_eq!(watcher["globPattern"]["pattern"], "**/*.langmeld");
    assert_eq!(
        watcher["globPattern"]["baseUri"],
        url::Url::from_directory_path(fs::canonicalize(&project).unwrap())
            .unwrap()
            .as_str()
    );
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":registration["id"],"result":null}),
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":root_uri,"languageId":"langmeld","version":1,"text":root_source}}}),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":6,"method":"textDocument/documentSymbol","params":{"textDocument":{"uri":root_uri}}}),
    );
    let symbols = receive_lsp_message(&receiver)["result"].clone();
    let symbol_names = symbols
        .as_array()
        .unwrap()
        .iter()
        .map(|symbol| symbol["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(symbol_names.contains(&"Widget"));
    assert!(symbol_names.contains(&"constructs/statements.langmeld"));
    assert!(symbol_names.contains(&"program"));

    let completion_position = root_source.find("allow \"").unwrap() + "allow \"".len();
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{"textDocument":{"uri":root_uri},"position":{"line":0,"character":completion_position}}}),
    );
    let initial_items = receive_lsp_message(&receiver)["result"].clone();
    assert!(
        initial_items
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "print")
    );
    assert!(
        !initial_items
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "render")
    );

    let module_source = fs::read_to_string(&module).unwrap();
    fs::write(
        &module,
        format!("{module_source} construct \"render\" {{ syntax {{ literal \"render\"; }} }}"),
    )
    .unwrap();
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":module_uri,"type":2}]}}),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":3,"method":"textDocument/completion","params":{"textDocument":{"uri":root_uri},"position":{"line":0,"character":completion_position}}}),
    );
    let updated_items = receive_lsp_message(&receiver)["result"].clone();
    assert!(
        updated_items
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "render")
    );

    let incomplete_module = format!(
        "{} construct \"preview\" {{ syntax {{ literal \"preview\"; }} }} construct \"unfinished",
        fs::read_to_string(&module).unwrap()
    );
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":module_uri,"languageId":"langmeld","version":1,"text":incomplete_module}}}),
    );
    for _ in 0..2 {
        assert_eq!(
            receive_lsp_message(&receiver)["method"],
            "textDocument/publishDiagnostics"
        );
    }
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":4,"method":"textDocument/completion","params":{"textDocument":{"uri":root_uri},"position":{"line":0,"character":completion_position}}}),
    );
    let incomplete_items = receive_lsp_message(&receiver)["result"].clone();
    assert!(
        incomplete_items
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "preview"),
        "completions should include declarations from the current incomplete buffer"
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
fn project_definitions_lsp_navigates_construct_and_category_references() {
    let parent = TestDirectory::new();
    let project = parent.path().join("project");
    write_widget_project(&project);
    let root = project.join("language/language.langmeld");
    let module = project.join("language/constructs/statements.langmeld");
    let root_source = r#"language "Widget" { import "constructs/statements.langmeld"; construct "program" { syntax { literal "program"; } block "{" "}" { allow "print"; allow_category "statement"; } } }"#;
    let module_source = r#"construct "print" { syntax { literal "print"; identifier "name"; literal ";"; } implements "statement"; } construct "say" { syntax { literal "say"; } implements "statement"; }"#;
    fs::write(&root, root_source).unwrap();
    fs::write(&module, module_source).unwrap();
    let root_uri = url::Url::from_file_path(&root).unwrap().to_string();
    let module_uri = url::Url::from_file_path(&module).unwrap().to_string();
    let (mut child, mut stdin, receiver) = start_lsp(&["lsp", "--definitions"], Some(&project));

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    let initialized = receive_lsp_message(&receiver);
    assert_eq!(
        initialized["result"]["capabilities"]["definitionProvider"],
        true
    );
    assert_eq!(
        initialized["result"]["capabilities"]["referencesProvider"],
        true
    );
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":root_uri,"languageId":"langmeld","version":1,"text":root_source}}}),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );

    let construct_offset = root_source.find("allow \"print\"").unwrap() + "allow \"p".len();
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{"textDocument":{"uri":root_uri},"position":{"line":0,"character":construct_offset}}}),
    );
    let definition = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(definition.as_array().unwrap().len(), 1);
    assert_eq!(
        canonical_file_uri(definition[0]["uri"].as_str().unwrap()),
        canonical_file_uri(&module_uri)
    );
    let print_span = module_source.find("\"print\"").unwrap();
    assert_eq!(definition[0]["range"]["start"]["character"], print_span);
    assert_eq!(
        definition[0]["range"]["end"]["character"],
        print_span + "\"print\"".len()
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":3,"method":"textDocument/references","params":{"textDocument":{"uri":root_uri},"position":{"line":0,"character":construct_offset},"context":{"includeDeclaration":true}}}),
    );
    let references = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(references.as_array().unwrap().len(), 2);
    assert!(references.as_array().unwrap().iter().any(|location| {
        canonical_file_uri(location["uri"].as_str().unwrap()) == canonical_file_uri(&root_uri)
    }));
    assert!(references.as_array().unwrap().iter().any(|location| {
        canonical_file_uri(location["uri"].as_str().unwrap()) == canonical_file_uri(&module_uri)
    }));

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":module_uri,"languageId":"langmeld","version":1,"text":module_source}}}),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );
    let declaration_offset =
        module_source.find("construct \"print\"").unwrap() + "construct \"p".len();
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":6,"method":"textDocument/references","params":{"textDocument":{"uri":module_uri},"position":{"line":0,"character":declaration_offset},"context":{"includeDeclaration":false}}}),
    );
    let mut response = receive_lsp_message(&receiver);
    while response["id"] != 6 {
        response = receive_lsp_message(&receiver);
    }
    let uses = response["result"].clone();
    assert_eq!(uses.as_array().unwrap().len(), 1);
    assert_eq!(uses[0]["uri"], root_uri);

    let category_offset =
        root_source.find("allow_category \"statement\"").unwrap() + "allow_category \"s".len();
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":4,"method":"textDocument/definition","params":{"textDocument":{"uri":root_uri},"position":{"line":0,"character":category_offset}}}),
    );
    let category_definitions = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(category_definitions.as_array().unwrap().len(), 2);
    assert!(
        category_definitions
            .as_array()
            .unwrap()
            .iter()
            .all(|location| {
                canonical_file_uri(location["uri"].as_str().unwrap())
                    == canonical_file_uri(&module_uri)
            })
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
fn project_definitions_lsp_reanalyzes_all_open_buffers_and_reverts_closed_overlays() {
    let parent = TestDirectory::new();
    let project = parent.path().join("project with spaces");
    write_widget_project(&project);
    let module = project.join("language/constructs/statements.langmeld");
    let root = project.join("language/language.langmeld");
    let module_uri = url::Url::from_file_path(&module).unwrap().to_string();
    let root_uri = url::Url::from_file_path(&root).unwrap().to_string();
    let (mut child, mut stdin, receiver) = start_lsp(&["lsp", "--definitions"], Some(&project));

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    assert_eq!(receive_lsp_message(&receiver)["id"], 1);
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );

    let root_allows_missing = "language \"Widget\" { import \"constructs/statements.langmeld\"; construct \"program\" { syntax { literal \"program\"; } block \"{\" \"}\" { allow \"missing\"; } } }";
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":root_uri,"languageId":"langmeld","version":1,"text":root_allows_missing}}}),
    );
    let root_diagnostics = receive_lsp_message(&receiver);
    assert_eq!(root_diagnostics["params"]["uri"], root_uri);
    assert_eq!(
        root_diagnostics["params"]["diagnostics"][0]["code"],
        "LM-DEF-004"
    );

    let module_defines_missing = "construct \"missing\" { syntax { literal \"missing\"; } }";
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":module_uri,"languageId":"langmeld","version":1,"text":module_defines_missing}}}),
    );
    let opened_module_messages = [
        receive_lsp_message(&receiver),
        receive_lsp_message(&receiver),
    ];
    let root_after_module_open = opened_module_messages
        .iter()
        .find(|message| message["params"]["uri"] == root_uri)
        .expect("opening an imported buffer republishes root diagnostics");
    assert!(
        root_after_module_open["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let root_allows_print = "language \"Widget\" { import \"constructs/statements.langmeld\"; construct \"program\" { syntax { literal \"program\"; } block \"{\" \"}\" { allow \"print\"; } } }";
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":root_uri,"version":2},"contentChanges":[{"text":root_allows_print}]}}),
    );
    let changed_root_messages = [
        receive_lsp_message(&receiver),
        receive_lsp_message(&receiver),
    ];
    let root_after_change = changed_root_messages
        .iter()
        .find(|message| message["params"]["uri"] == root_uri)
        .expect("changing the root republishes diagnostics for every open file");
    assert_eq!(
        root_after_change["params"]["diagnostics"][0]["code"],
        "LM-DEF-004"
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":module_uri}}}),
    );
    let closed_module_messages = [
        receive_lsp_message(&receiver),
        receive_lsp_message(&receiver),
    ];
    let root_after_module_close = closed_module_messages
        .iter()
        .find(|message| message["params"]["uri"] == root_uri)
        .expect("closing an imported buffer republishes remaining project diagnostics");
    assert!(
        root_after_module_close["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}),
    );
    assert_eq!(receive_lsp_message(&receiver)["id"], 2);
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"exit","params":null}),
    );
    drop(stdin);
    assert!(child.0.wait().unwrap().success());
}
