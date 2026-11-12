use super::*;

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
fn project_definitions_lsp_publishes_a_legacy_lm_import_warning() {
    let parent = TestDirectory::new();
    let project = parent.path().join("legacy-project");
    write_widget_project(&project);
    let root = project.join("language/language.langmeld");
    let module = project.join("language/constructs/statements.langmeld");
    let legacy_module = project.join("language/constructs/statements.lm");
    fs::rename(&module, &legacy_module).expect("legacy module should be renamed");
    fs::write(
        &root,
        r#"language "Widget" {
    import "constructs/statements.lm";
    construct "program" {
        syntax { literal "program"; }
        block "{" "}" { allow "print"; }
    }
}"#,
    )
    .expect("root definition should import the legacy module");
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
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":root_uri,"languageId":"langmeld","version":1,"text":fs::read_to_string(&root).unwrap()}}}),
    );

    let published = receive_lsp_message(&receiver);
    assert_eq!(published["method"], "textDocument/publishDiagnostics");
    let warning = published["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|diagnostic| diagnostic["code"] == "LM-DEF-W001")
        .expect("legacy import warning should be published");
    assert_eq!(warning["severity"], 2);
    assert!(warning["message"].as_str().unwrap().contains("`.langmeld`"));

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

#[test]
fn project_definitions_lsp_publishes_a_legacy_lm_root_warning() {
    let parent = TestDirectory::new();
    let project = parent.path().join("legacy-root-project");
    write_widget_project(&project);

    let root = project.join("language/language.langmeld");
    let legacy_root = project.join("language/language.lm");
    fs::rename(&root, &legacy_root).expect("root should be renamed to its legacy extension");
    fs::write(
        project.join("langmeld.toml"),
        "format = 1\ndefinition = \"language/language.lm\"\n",
    )
    .expect("manifest should point to the legacy root");

    let root_uri = url::Url::from_file_path(&legacy_root).unwrap().to_string();
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
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":root_uri,"languageId":"langmeld","version":1,"text":fs::read_to_string(&legacy_root).unwrap()}}}),
    );

    let published = receive_lsp_message(&receiver);
    assert_eq!(published["method"], "textDocument/publishDiagnostics");
    let warning = published["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|diagnostic| diagnostic["code"] == "LM-DEF-W001")
        .expect("legacy root warning should be published");
    assert_eq!(warning["severity"], 2);
    assert!(warning["message"].as_str().unwrap().contains("`.langmeld`"));

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

#[test]
fn project_definitions_lsp_resolves_a_new_unsaved_imported_buffer() {
    let parent = TestDirectory::new();
    let project = parent.path().join("project");
    write_widget_project(&project);
    let module = project.join("language/constructs/statements.langmeld");
    fs::remove_file(&module).unwrap();
    let root = project.join("language/language.langmeld");
    let module_uri = url::Url::from_file_path(&module).unwrap().to_string();
    let root_uri = url::Url::from_file_path(&root).unwrap().to_string();
    let module_source =
        "construct \"print\" { syntax { literal \"print\"; identifier \"name\"; literal \";\"; } }";
    let root_source = "language \"Widget\" { import \"constructs/statements.langmeld\"; construct \"program\" { syntax { literal \"program\"; } block \"{\" \"}\" { allow \"print\"; } } }";
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
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":module_uri,"languageId":"langmeld","version":1,"text":module_source}}}),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":root_uri,"languageId":"langmeld","version":1,"text":root_source}}}),
    );
    let diagnostics = [
        receive_lsp_message(&receiver),
        receive_lsp_message(&receiver),
    ];
    assert!(diagnostics.iter().all(|message| {
        message["method"] == "textDocument/publishDiagnostics"
            && message["params"]["diagnostics"]
                .as_array()
                .unwrap()
                .is_empty()
    }));

    let completion_position = root_source.find("allow \"").unwrap() + "allow \"".len();
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{"textDocument":{"uri":root_uri},"position":{"line":0,"character":completion_position}}}),
    );
    let completion = receive_lsp_message(&receiver)["result"].clone();
    assert!(
        completion
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "print")
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":module_uri}}}),
    );
    let closed_diagnostics = [
        receive_lsp_message(&receiver),
        receive_lsp_message(&receiver),
    ];
    let root_diagnostics = closed_diagnostics
        .iter()
        .find(|message| message["params"]["uri"] == root_uri)
        .expect("closing the unsaved import republishes root diagnostics");
    assert!(
        !root_diagnostics["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":3,"method":"textDocument/completion","params":{"textDocument":{"uri":root_uri},"position":{"line":0,"character":completion_position}}}),
    );
    let completion_after_close = receive_lsp_message(&receiver)["result"].clone();
    assert!(
        !completion_after_close
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "print")
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":4,"method":"shutdown","params":null}),
    );
    assert_eq!(receive_lsp_message(&receiver)["id"], 4);
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"exit","params":null}),
    );
    drop(stdin);
    assert!(child.0.wait().unwrap().success());
}
