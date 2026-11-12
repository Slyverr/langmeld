use super::*;
use serde_json::json;

#[test]
fn serves_diagnostics_completion_navigation_and_incremental_document_changes() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let backend = Backend::default();
    let mut closed_sources = SourceDatabase::default();
    let closed_source_id = closed_sources
        .insert("file:///closed.test", "declaration")
        .unwrap();
    backend.0.lock().unwrap().insert(
        "file:///closed.test".to_owned(),
        closed_sources.get(closed_source_id).unwrap().clone(),
    );
    let observed = Backend(backend.0.clone());
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, backend);
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
                "general": { "positionEncodings": ["utf-16", "utf-8"] },
                "textDocument": {
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "documentSymbol": { "hierarchicalDocumentSymbolSupport": true }
                }
            }
        }),
    );
    let initialized = receive_response(&client);
    assert_eq!(initialized["capabilities"]["positionEncoding"], "utf-16");
    assert_eq!(initialized["capabilities"]["textDocumentSync"], 2);
    assert!(initialized["capabilities"].get("workspace").is_none());
    assert!(initialized["capabilities"]["completionProvider"].is_object());
    assert_eq!(initialized["capabilities"]["definitionProvider"], true);
    assert_eq!(initialized["capabilities"]["referencesProvider"], true);
    assert_eq!(
        initialized["capabilities"]["semanticTokensProvider"]["legend"]["tokenTypes"],
        json!(["variable"])
    );
    assert_eq!(
        initialized["capabilities"]["semanticTokensProvider"]["full"],
        true
    );
    assert_eq!(
        initialized["capabilities"]["documentFormattingProvider"],
        true
    );
    assert_eq!(initialized["capabilities"]["hoverProvider"], true);
    assert_eq!(initialized["capabilities"]["renameProvider"], true);
    assert_eq!(initialized["capabilities"]["documentSymbolProvider"], true);
    assert_eq!(initialized["capabilities"]["workspaceSymbolProvider"], true);
    send_notification(&client, "initialized", json!({}));

    let uri = "file:///main.test";
    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "test",
                "version": 1,
                "text": "😀bad"
            }
        }),
    );
    let published = receive_diagnostics(&client);
    assert_eq!(published["diagnostics"][0]["code"], "TEST-001");
    assert_eq!(
        published["diagnostics"][0]["range"]["start"]["character"],
        0
    );
    assert_eq!(published["diagnostics"][0]["range"]["end"]["character"], 2);

    send_request(
        &client,
        30,
        "workspace/symbol",
        json!({ "query": "Record" }),
    );
    let workspace_symbols = receive_response(&client);
    assert_eq!(workspace_symbols[0]["name"], "Record");
    assert_eq!(workspace_symbols[0]["kind"], 23);
    assert_eq!(
        workspace_symbols[0]["location"]["uri"],
        "file:///closed.test"
    );
    assert_eq!(
        workspace_symbols[0]["location"]["range"]["end"]["character"],
        11
    );

    send_request(
        &client,
        2,
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    );
    let symbols = receive_response(&client);
    assert_eq!(symbols[0]["name"], "root");
    assert_eq!(symbols[0]["range"]["end"]["character"], 5);
    assert_eq!(symbols[0]["selectionRange"]["end"]["character"], 2);
    assert_eq!(symbols[0]["children"][0]["name"], "bad");
    assert_eq!(symbols[0]["children"][0]["range"]["start"]["character"], 2);

    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": "file:///invalid-symbol.test",
                "languageId": "test",
                "version": 1,
                "text": "😀bad"
            }
        }),
    );
    receive_diagnostics(&client);
    send_request(
        &client,
        30,
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": "file:///invalid-symbol.test" } }),
    );
    let symbols = receive_response(&client);
    assert_eq!(symbols.as_array().unwrap().len(), 2);
    assert_eq!(symbols[0]["name"], "valid");
    assert_eq!(symbols[1]["children"].as_array().unwrap().len(), 1);
    assert_eq!(symbols[1]["children"][0]["name"], "valid-child");

    send_request(
        &client,
        3,
        "textDocument/semanticTokens/full",
        json!({ "textDocument": { "uri": uri } }),
    );
    let tokens = receive_response(&client);
    assert_eq!(tokens["data"], json!([0, 0, 2, 0, 1, 0, 2, 3, 0, 0]));

    send_request(
        &client,
        4,
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": uri },
            "options": { "tabSize": 2, "insertSpaces": true }
        }),
    );
    let edits = receive_response(&client);
    assert_eq!(
        edits[0]["range"]["start"],
        json!({ "line": 0, "character": 0 })
    );
    assert_eq!(
        edits[0]["range"]["end"],
        json!({ "line": 0, "character": 5 })
    );
    assert_eq!(edits[0]["newText"], "  😀bad");

    send_request(
        &client,
        5,
        "textDocument/hover",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 }
        }),
    );
    let hover = receive_response(&client);
    assert_eq!(hover["contents"]["kind"], "markdown");
    assert_eq!(hover["contents"]["value"], "**bad**");
    assert_eq!(hover["range"]["start"]["character"], 2);
    assert_eq!(hover["range"]["end"]["character"], 5);

    send_request(
        &client,
        6,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 },
            "newName": "renamed"
        }),
    );
    let rename = receive_response(&client);
    assert_eq!(rename["changes"][uri].as_array().unwrap().len(), 2);
    assert_eq!(rename["changes"][uri][0]["range"]["start"]["character"], 0);
    assert_eq!(rename["changes"][uri][0]["newText"], "renamed");
    assert_eq!(rename["changes"][uri][1]["range"]["start"]["character"], 2);

    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": "file:///other.test",
                "languageId": "test",
                "version": 1,
                "text": "other"
            }
        }),
    );
    receive_diagnostics(&client);
    send_request(
        &client,
        7,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 },
            "newName": "cross"
        }),
    );
    let cross_file_rename = receive_response(&client);
    assert_eq!(
        cross_file_rename["changes"][uri].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        cross_file_rename["changes"]["file:///other.test"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(cross_file_rename["changes"][uri][0]["newText"], "cross");
    assert_eq!(
        cross_file_rename["changes"]["file:///other.test"][0]["newText"],
        "cross"
    );

    send_request(
        &client,
        20,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 },
            "newName": "closed"
        }),
    );
    let closed_file_rename = receive_response(&client);
    assert_eq!(
        closed_file_rename["changes"]["file:///closed.test"][0]["range"]["end"]["character"],
        11
    );

    send_request(
        &client,
        8,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 },
            "newName": "overlap"
        }),
    );
    assert!(receive_response(&client).is_null());

    send_request(
        &client,
        9,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 },
            "newName": "empty"
        }),
    );
    assert!(receive_response(&client).is_null());

    send_request(
        &client,
        10,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 },
            "newName": "missing"
        }),
    );
    assert!(receive_response(&client).is_null());

    send_request(
        &client,
        11,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 }
        }),
    );
    let completion = receive_response(&client);
    assert_eq!(completion.as_array().unwrap().len(), 2);
    assert!(
        completion
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["label"] != "Function call")
    );
    assert_eq!(completion[0]["label"], "Good value");
    assert_eq!(completion[0]["detail"], "Suggested item");
    assert_eq!(completion[0]["textEdit"]["newText"], "good");
    assert_eq!(completion[0]["insertTextFormat"], 1);
    assert_eq!(completion[0]["filterText"], "good");
    assert_eq!(completion[1]["label"], "Great value");
    assert_eq!(completion[1]["kind"], 22);
    assert_eq!(completion[1]["textEdit"]["newText"], "great");
    assert_eq!(completion[1]["filterText"], "preferred");
    assert_eq!(completion[0]["textEdit"]["range"]["start"]["character"], 2);
    assert_eq!(completion[0]["textEdit"]["range"]["end"]["character"], 5);

    send_request(
        &client,
        12,
        "textDocument/definition",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 }
        }),
    );
    let definition = receive_response(&client);
    assert_eq!(definition[0]["uri"], uri);
    assert_eq!(definition[0]["range"]["end"]["character"], 2);

    send_request(
        &client,
        13,
        "textDocument/references",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 },
            "context": { "includeDeclaration": true }
        }),
    );
    let references = receive_response(&client);
    assert_eq!(references.as_array().unwrap().len(), 1);

    send_notification(
        &client,
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [
                {
                    "range": {
                        "start": { "line": 0, "character": 2 },
                        "end": { "line": 0, "character": 5 }
                    },
                    "text": "good"
                },
                {
                    "range": {
                        "start": { "line": 0, "character": 6 },
                        "end": { "line": 0, "character": 6 }
                    },
                    "text": "!"
                }
            ]
        }),
    );
    let changed = receive_diagnostics(&client);
    assert!(changed["diagnostics"].as_array().unwrap().is_empty());
    assert_eq!(changed["version"], 2);
    assert_eq!(
        observed.0.lock().unwrap().get(uri).unwrap().text(),
        "😀good!"
    );

    send_request(
        &client,
        14,
        "textDocument/semanticTokens/full",
        json!({ "textDocument": { "uri": uri } }),
    );
    let tokens = receive_response(&client);
    assert_eq!(tokens["data"], json!([0, 0, 2, 0, 1, 0, 2, 3, 0, 0]));

    // Older document versions must not replace the current snapshot or publish
    // stale diagnostics.
    send_notification(
        &client,
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": uri, "version": 1 },
            "contentChanges": [{ "text": "bad" }]
        }),
    );

    send_notification(
        &client,
        "textDocument/didClose",
        json!({ "textDocument": { "uri": uri } }),
    );
    let closed = receive_diagnostics(&client);
    assert!(closed["diagnostics"].as_array().unwrap().is_empty());

    send_request(&client, 15, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}

#[test]
fn returns_snippet_completions_when_the_client_supports_them() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, Backend::default());
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
    receive_response(&client);
    send_notification(&client, "initialized", json!({}));

    let uri = "file:///snippets.test";
    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "test",
                "version": 1,
                "text": "😀bad"
            }
        }),
    );
    receive_diagnostics(&client);

    send_request(
        &client,
        2,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 }
        }),
    );
    let completion = receive_response(&client);
    let snippet = completion
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["label"] == "Function call")
        .expect("snippet completion should be included");
    assert_eq!(snippet["kind"], 3);
    assert_eq!(snippet["insertTextFormat"], 2);
    assert_eq!(snippet["textEdit"]["newText"], "call(${1:argument})$0");

    send_request(&client, 3, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}

#[test]
fn incremental_document_changes_use_negotiated_utf8_and_utf32_positions() {
    assert_incremental_edit_uses_position_encoding("utf-8", 4, 7);
    assert_incremental_edit_uses_position_encoding("utf-32", 1, 4);
}

fn assert_incremental_edit_uses_position_encoding(
    encoding: &str,
    start_character: u32,
    end_character: u32,
) {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let backend = Backend::default();
    let observed = Backend(backend.0.clone());
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, backend);
        server_sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    send_request(
        &client,
        1,
        "initialize",
        json!({ "capabilities": { "general": { "positionEncodings": [encoding] } } }),
    );
    let initialized = receive_response(&client);
    assert_eq!(initialized["capabilities"]["positionEncoding"], encoding);
    send_notification(&client, "initialized", json!({}));

    let uri = format!("file:///{encoding}.test");
    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "test",
                "version": 1,
                "text": "😀bad"
            }
        }),
    );
    receive_diagnostics(&client);

    send_notification(
        &client,
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [{
                "range": {
                    "start": { "line": 0, "character": start_character },
                    "end": { "line": 0, "character": end_character }
                },
                "text": "ok"
            }]
        }),
    );
    let changed = receive_diagnostics(&client);
    assert!(changed["diagnostics"].as_array().unwrap().is_empty());
    assert_eq!(observed.0.lock().unwrap().get(&uri).unwrap().text(), "😀ok");

    send_request(&client, 2, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}
