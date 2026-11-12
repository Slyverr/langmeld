use super::*;
use serde_json::json;

#[test]
fn declarative_backend_provides_scoped_completion_over_lsp() {
    let language = LanguageSpec::new(
        "Demo",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["print"],
            ),
            ConstructSpec::new(
                "print",
                [
                    PatternPart::literal("print"),
                    PatternPart::choice_with_completion_kind(
                        "speed",
                        ["fast", "slow"],
                        CompletionKind::Keyword,
                    ),
                    PatternPart::literal(";"),
                ],
            ),
        ],
    )
    .expect("valid test grammar");
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, DeclarativeBackend::new(language));
        server_sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    send_request(&client, 1, "initialize", json!({"capabilities": {}}));
    let initialized = receive_response(&client);
    assert!(initialized["capabilities"]["completionProvider"].is_object());
    assert!(initialized["capabilities"]["definitionProvider"].is_null());
    assert!(initialized["capabilities"]["semanticTokensProvider"].is_null());
    assert!(initialized["capabilities"]["documentFormattingProvider"].is_null());
    assert!(initialized["capabilities"]["hoverProvider"].is_null());
    assert!(initialized["capabilities"]["renameProvider"].is_null());
    assert!(initialized["capabilities"]["documentSymbolProvider"].is_null());
    send_notification(&client, "initialized", json!({}));

    let uri = "file:///declarative.langmeld";
    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "demo",
                "version": 1,
                "text": "program { pri"
            }
        }),
    );
    let diagnostics = receive_diagnostics(&client);
    assert_eq!(diagnostics["uri"], uri);

    send_request(
        &client,
        2,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 13 }
        }),
    );
    let completions = receive_response(&client);
    let labels = completions
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["label"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(labels, ["print"]);

    let choice_source = "program { print f";
    send_notification(
        &client,
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [{ "text": choice_source }]
        }),
    );
    receive_diagnostics(&client);
    send_request(
        &client,
        3,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": choice_source.len() }
        }),
    );
    let completions = receive_response(&client);
    let choices = completions
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["label"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(choices, ["fast"]);
    assert_eq!(completions[0]["kind"], 14);
    assert_eq!(
        completions[0]["textEdit"]["range"]["start"]["character"],
        16
    );
    assert_eq!(completions[0]["textEdit"]["range"]["end"]["character"], 17);

    let complete_keyword_source = "program { print";
    send_notification(
        &client,
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": uri, "version": 3 },
            "contentChanges": [{ "text": complete_keyword_source }]
        }),
    );
    receive_diagnostics(&client);
    send_request(
        &client,
        4,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": complete_keyword_source.len() }
        }),
    );
    let completions = receive_response(&client);
    let labels = completions
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["label"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(labels, ["fast", "slow"]);
    assert_eq!(
        completions[0]["textEdit"]["range"]["start"]["character"],
        complete_keyword_source.len()
    );
    assert_eq!(
        completions[0]["textEdit"]["range"]["end"]["character"],
        complete_keyword_source.len()
    );

    send_request(&client, 5, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}
