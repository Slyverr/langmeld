use super::*;

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
