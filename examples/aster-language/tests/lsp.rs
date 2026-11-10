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

    send_request(&client, 1, "initialize", json!({"capabilities": {}}));
    let initialized = receive_response(&client);
    assert!(initialized["capabilities"]["completionProvider"].is_null());
    assert_eq!(initialized["capabilities"]["definitionProvider"], true);
    assert_eq!(initialized["capabilities"]["referencesProvider"], true);
    send_notification(&client, "initialized", json!({}));

    let uri = "file:///main.aster";
    let text = "fn add(value: i32) -> i32 { return value; }\nfn main() -> i32 { return add(1); }\nfn invalid() -> i32 { return missing; }";
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

    let second_line = text.find('\n').unwrap() + 1;
    let add_call = text.rfind("add(1)").unwrap() - second_line;
    let add_declaration = text.find("add(value").unwrap();
    send_request(
        &client,
        2,
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
        3,
        "textDocument/references",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 1, "character": add_call },
            "context": { "includeDeclaration": true }
        }),
    );
    let references = receive_response(&client);
    assert_eq!(references.as_array().unwrap().len(), 2);

    send_request(&client, 4, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}
