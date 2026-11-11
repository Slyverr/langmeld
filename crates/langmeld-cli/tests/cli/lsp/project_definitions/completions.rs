use super::*;

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
