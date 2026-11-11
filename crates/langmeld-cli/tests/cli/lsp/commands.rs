use super::*;
use serde_json::json;

#[test]
fn lsp_command_loads_the_manifest_and_serves_scoped_completion() {
    let parent = TestDirectory::new();
    let project = parent.path();
    write_widget_project(project);
    let manifest_path = project.join("langmeld.toml");
    let manifest_path = manifest_path.to_str().unwrap();
    let (mut child, mut stdin, receiver) =
        start_lsp(&["lsp", "--manifest-path", manifest_path], None);

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    let initialized = receive_lsp_message(&receiver);
    assert_eq!(initialized["id"], 1);
    assert!(initialized["result"]["capabilities"]["completionProvider"].is_object());

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":"file:///main.widget","languageId":"widget","version":1,"text":"program { pri"}
            }
        }),
    );
    let diagnostics = receive_lsp_message(&receiver);
    assert_eq!(diagnostics["method"], "textDocument/publishDiagnostics");

    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{
                "textDocument":{"uri":"file:///main.widget"},"position":{"line":0,"character":13}
            }
        }),
    );
    let completion = receive_lsp_message(&receiver);
    assert_eq!(completion["id"], 2);
    assert_eq!(completion["result"][0]["label"], "print");

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
fn lsp_definitions_mode_serves_context_aware_langmeld_completions() {
    let parent = TestDirectory::new();
    let (mut child, mut stdin, receiver) =
        start_lsp(&["lsp", "--definitions"], Some(parent.path()));

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    let initialized = receive_lsp_message(&receiver);
    assert!(initialized["result"]["capabilities"]["completionProvider"].is_object());
    assert_eq!(
        initialized["result"]["capabilities"]["documentFormattingProvider"],
        true
    );
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );

    let root_source = "lan";
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":"file:///language.langmeld","languageId":"langmeld","version":1,"text":root_source}
            }
        }),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{
                "textDocument":{"uri":"file:///language.langmeld"},"position":{"line":0,"character":3}
            }
        }),
    );
    let root_completions = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(root_completions[0]["label"], "language");
    assert_eq!(root_completions.as_array().unwrap().len(), 1);

    let nested_source = "language \"Toy\" { construct \"item\" { syntax { lit";
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":"file:///constructs.langmeld","languageId":"langmeld","version":1,"text":nested_source}
            }
        }),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","id":3,"method":"textDocument/completion","params":{
                "textDocument":{"uri":"file:///constructs.langmeld"},"position":{"line":0,"character":nested_source.len()}
            }
        }),
    );
    let nested_completions = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(nested_completions.as_array().unwrap().len(), 1);
    assert_eq!(nested_completions[0]["label"], "literal");

    let construct_source = "language \"Toy\" { construct \"item\" { comp";
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":"file:///construct-body.langmeld","languageId":"langmeld","version":1,"text":construct_source}
            }
        }),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","id":4,"method":"textDocument/completion","params":{
                "textDocument":{"uri":"file:///construct-body.langmeld"},"position":{"line":0,"character":construct_source.len()}
            }
        }),
    );
    let construct_completions = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(construct_completions.as_array().unwrap().len(), 1);
    assert_eq!(construct_completions[0]["label"], "completion");

    let choice_source = "language \"Toy\" { construct \"item\" { syntax { choice \"mode\" { ";
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":"file:///choice.langmeld","languageId":"langmeld","version":1,"text":choice_source}
            }
        }),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","id":5,"method":"textDocument/completion","params":{
                "textDocument":{"uri":"file:///choice.langmeld"},"position":{"line":0,"character":choice_source.len()}
            }
        }),
    );
    let choice_completions = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(
        choice_completions
            .as_array()
            .unwrap()
            .iter()
            .map(|candidate| candidate["label"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["completion", "value", "}"]
    );

    let kind_source = "language \"Toy\" { construct \"item\" { completion key";
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":"file:///completion-kind.langmeld","languageId":"langmeld","version":1,"text":kind_source}
            }
        }),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","id":6,"method":"textDocument/completion","params":{
                "textDocument":{"uri":"file:///completion-kind.langmeld"},"position":{"line":0,"character":kind_source.len()}
            }
        }),
    );
    let kind_completions = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(kind_completions.as_array().unwrap().len(), 1);
    assert_eq!(kind_completions[0]["label"], "keyword");
    assert_eq!(kind_completions[0]["kind"], 20);

    let format_source = "language \"Toy\"{construct \"item\"{syntax{literal \"item\";}}}";
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":"file:///format.langmeld","languageId":"langmeld","version":1,"text":format_source}
            }
        }),
    );
    assert_eq!(
        receive_lsp_message(&receiver)["method"],
        "textDocument/publishDiagnostics"
    );
    send_lsp_message(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","id":7,"method":"textDocument/formatting","params":{
                "textDocument":{"uri":"file:///format.langmeld"},
                "options":{"tabSize":2,"insertSpaces":true}
            }
        }),
    );
    let edits = receive_lsp_message(&receiver)["result"].clone();
    assert_eq!(edits.as_array().unwrap().len(), 1);
    assert_eq!(
        edits[0]["newText"],
        "language \"Toy\" {\n  construct \"item\" {\n    syntax {\n      literal \"item\";\n    }\n  }\n}\n"
    );

    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":8,"method":"shutdown","params":null}),
    );
    assert_eq!(receive_lsp_message(&receiver)["id"], 8);
    send_lsp_message(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"exit","params":null}),
    );
    drop(stdin);
    assert!(child.0.wait().unwrap().success());
}
