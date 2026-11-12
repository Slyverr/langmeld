use std::{sync::mpsc, thread, time::Duration};

use langmeld_aster_language_example::AsterBackend;
use langmeld_lsp::run_connection;
use lsp_server::{Connection, Message, Notification, Request, RequestId};
use serde_json::{Value, json};

fn send_request(connection: &Connection, id: i32, method: &str, params: Value) {
    connection
        .sender
        .send(Message::Request(Request {
            id: RequestId::from(id),
            method: method.into(),
            params,
        }))
        .unwrap();
}

fn send_notification(connection: &Connection, method: &str, params: Value) {
    connection
        .sender
        .send(Message::Notification(Notification {
            method: method.into(),
            params,
        }))
        .unwrap();
}

fn receive(connection: &Connection) -> Message {
    connection
        .receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server should respond")
}

fn receive_response(connection: &Connection) -> Value {
    match receive(connection) {
        Message::Response(response) => response.response_result.expect("successful response"),
        message => panic!("expected response, got {message:?}"),
    }
}

fn decode_tokens(value: &Value) -> Vec<(u32, u32, u32, u32, u32)> {
    let mut line = 0;
    let mut character = 0;
    let data = value["data"]
        .as_array()
        .expect("semantic token data is an array");
    let (tokens, remainder) = data.as_chunks::<5>();
    assert!(
        remainder.is_empty(),
        "semantic token data uses groups of five"
    );
    tokens
        .iter()
        .map(|token| {
            let delta_line = token[0].as_u64().unwrap() as u32;
            let delta_start = token[1].as_u64().unwrap() as u32;
            line += delta_line;
            character = if delta_line == 0 {
                character + delta_start
            } else {
                delta_start
            };
            (
                line,
                character,
                token[2].as_u64().unwrap() as u32,
                token[3].as_u64().unwrap() as u32,
                token[4].as_u64().unwrap() as u32,
            )
        })
        .collect()
}

#[test]
fn exposes_host_semantic_diagnostics_and_resolved_navigation() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, AsterBackend);
        server_sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    send_request(
        &client,
        1,
        "initialize",
        json!({
            "capabilities": {
                "textDocument": {
                    "completion": {
                        "completionItem": { "snippetSupport": true }
                    }
                }
            }
        }),
    );
    let initialized = receive_response(&client);
    assert!(!initialized["capabilities"]["completionProvider"].is_null());
    assert_eq!(initialized["capabilities"]["definitionProvider"], true);
    assert_eq!(initialized["capabilities"]["referencesProvider"], true);
    assert_eq!(initialized["capabilities"]["renameProvider"], true);
    assert_eq!(initialized["capabilities"]["documentSymbolProvider"], true);
    assert_eq!(
        initialized["capabilities"]["semanticTokensProvider"]["legend"]["tokenTypes"],
        json!(["function", "parameter", "variable"])
    );
    send_notification(&client, "initialized", json!({}));

    let uri = "file:///main.aster";
    let text = "fn add(value: i32) -> i32 { return value; }\nfn main() -> i32 { return add(1); }\nfn invalid() -> i32 { return missing; }\n";
    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "aster",
                "version": 1,
                "text": text
            }
        }),
    );
    match receive(&client) {
        Message::Notification(notification) => {
            assert_eq!(notification.method, "textDocument/publishDiagnostics");
            assert!(
                notification.params["diagnostics"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|diagnostic| diagnostic["code"] == "ASTER-SEM-002")
            );
        }
        message => panic!("expected diagnostics, got {message:?}"),
    }

    send_request(
        &client,
        2,
        "textDocument/semanticTokens/full",
        json!({ "textDocument": { "uri": uri } }),
    );
    let tokens = decode_tokens(&receive_response(&client));
    let token_at = |offset: usize| {
        let line = text[..offset].matches('\n').count() as u32;
        let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
        let character = text[line_start..offset].chars().count() as u32;
        tokens
            .iter()
            .find(|token| token.0 == line && token.1 == character)
            .copied()
    };
    let add_declaration = token_at(text.find("add(value").unwrap()).unwrap();
    assert_eq!((add_declaration.3, add_declaration.4), (0, 1));
    let parameter_declaration = token_at(text.find("value:").unwrap()).unwrap();
    assert_eq!((parameter_declaration.3, parameter_declaration.4), (1, 1));
    let parameter_use = token_at(text.find("return value").unwrap() + "return ".len()).unwrap();
    assert_eq!((parameter_use.3, parameter_use.4), (1, 0));
    let add_reference = token_at(text.rfind("add(1)").unwrap()).unwrap();
    assert_eq!((add_reference.3, add_reference.4), (0, 0));

    let second_line = text.find('\n').unwrap() + 1;
    let add_call = text.rfind("add(1)").unwrap() - second_line;
    let add_declaration = text.find("add(value").unwrap();
    send_request(
        &client,
        3,
        "textDocument/definition",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": add_call }
        }),
    );
    let definition = receive_response(&client);
    assert_eq!(
        definition[0]["range"]["start"]["character"],
        add_declaration
    );

    send_request(
        &client,
        4,
        "textDocument/references",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": add_call },
            "context": { "includeDeclaration": true }
        }),
    );
    let references = receive_response(&client);
    assert_eq!(references.as_array().unwrap().len(), 2);

    send_request(
        &client,
        5,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": add_call },
            "newName": "calculate"
        }),
    );
    let rename = receive_response(&client);
    assert_eq!(rename["changes"][uri].as_array().unwrap().len(), 2);
    assert_eq!(rename["changes"][uri][0]["newText"], "calculate");
    assert_eq!(rename["changes"][uri][1]["newText"], "calculate");

    send_request(
        &client,
        6,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": add_call },
            "newName": "main"
        }),
    );
    assert!(receive_response(&client).is_null());

    send_request(
        &client,
        7,
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    );
    let symbols = receive_response(&client);
    let symbol_names = symbols
        .as_array()
        .unwrap()
        .iter()
        .map(|symbol| symbol["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(symbol_names, ["add", "value", "main", "invalid"]);
    assert_eq!(symbols[0]["kind"], 12);
    assert_eq!(symbols[0]["location"]["range"]["start"]["character"], 0);
    assert_eq!(
        symbols[0]["location"]["range"]["end"]["character"],
        text.find('\n').unwrap()
    );
    assert_eq!(symbols[1]["containerName"], "add");

    send_request(
        &client,
        8,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 3, "character": 0 }
        }),
    );
    let completions = receive_response(&client);
    assert_eq!(completions.as_array().unwrap().len(), 1);
    assert_eq!(completions[0]["label"], "Function declaration");
    assert_eq!(completions[0]["kind"], 15);
    assert_eq!(completions[0]["insertTextFormat"], 2);
    assert_eq!(
        completions[0]["textEdit"]["newText"],
        "fn ${1:name}(${2:parameter}: ${3:i32}) -> ${4:i32} {\n    $0\n}"
    );

    send_request(&client, 9, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}
