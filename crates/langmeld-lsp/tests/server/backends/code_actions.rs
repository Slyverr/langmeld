use super::*;
use lsp_types::{CodeActionOrCommand, Command};
use serde_json::json;

struct CodeActionsBackend;

impl LanguageBackend for CodeActionsBackend {
    type Document = ();

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        BackendDocument::new(source, (), Vec::new())
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            code_actions: true,
            rename: true,
            prepare_rename: true,
            ..BackendCapabilities::default()
        }
    }

    fn code_actions(
        &self,
        _source: &SourceFile,
        _document: &BackendDocument<Self::Document>,
        _params: &lsp_types::CodeActionParams,
        range: Span,
    ) -> Vec<CodeActionOrCommand> {
        assert_eq!(range, Span::new(4, 8));
        vec![CodeActionOrCommand::Command(Command::new(
            "UTF-8 byte span received".to_owned(),
            "langmeld.test".to_owned(),
            None,
        ))]
    }

    fn prepare_rename(
        &self,
        _source: &SourceFile,
        _document: &BackendDocument<Self::Document>,
        offset: usize,
    ) -> Option<RenamePreparation> {
        assert_eq!(offset, 4);
        Some(RenamePreparation::new(Span::new(4, 8), "name"))
    }
}

#[test]
fn code_action_callback_receives_validated_utf8_byte_range() {
    let (server_connection, client) = Connection::memory();
    let (server_sender, server_receiver) = mpsc::channel();
    let server_thread = thread::spawn(move || {
        let result = run_connection(server_connection, CodeActionsBackend);
        server_sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    send_request(&client, 1, "initialize", json!({ "capabilities": {} }));
    let initialize = receive_response(&client);
    assert_eq!(initialize["capabilities"]["codeActionProvider"], true);
    assert_eq!(
        initialize["capabilities"]["renameProvider"]["prepareProvider"],
        true
    );
    send_notification(&client, "initialized", json!({}));

    let uri = "file:///code-actions.lang";
    send_notification(
        &client,
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "test",
                "version": 1,
                "text": "😀name"
            }
        }),
    );
    let diagnostics = receive_diagnostics(&client);
    assert_eq!(diagnostics["uri"], uri);

    send_request(
        &client,
        2,
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": 0, "character": 2 },
                "end": { "line": 0, "character": 6 }
            },
            "context": { "diagnostics": [] }
        }),
    );
    let actions = receive_response(&client);
    assert_eq!(actions[0]["command"], "langmeld.test");
    assert_eq!(actions[0]["title"], "UTF-8 byte span received");

    send_request(
        &client,
        3,
        "textDocument/prepareRename",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 2 }
        }),
    );
    let selection = receive_response(&client);
    assert_eq!(selection["placeholder"], "name");
    assert_eq!(selection["range"]["start"]["character"], 2);
    assert_eq!(selection["range"]["end"]["character"], 6);

    send_request(&client, 4, "shutdown", Value::Null);
    let _ = receive_response(&client);
    send_notification(&client, "exit", Value::Null);
    drop(client);
    server_thread.join().unwrap();
    server_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("server should stop")
        .expect("server should stop cleanly");
}
