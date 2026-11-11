use super::*;

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
