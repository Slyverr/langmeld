use super::*;
use serde_json::json;

#[test]
fn language_definition_backend_advertises_grammar_trigger_characters() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, LanguageDefinitionBackend);
        server_sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    send_request(&client, 1, "initialize", json!({ "capabilities": {} }));
    let initialized = receive_response(&client);
    assert_eq!(
        initialized["capabilities"]["completionProvider"]["triggerCharacters"],
        json!([";", "{", "}"])
    );

    send_notification(&client, "initialized", json!({}));
    send_request(&client, 2, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}
