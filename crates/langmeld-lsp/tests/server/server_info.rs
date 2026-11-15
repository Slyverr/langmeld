use super::*;
use serde_json::json;

#[test]
fn reports_langmeld_server_info_by_default() {
    let (server_connection, client) = Connection::memory();
    let server_thread =
        thread::spawn(move || run_connection(server_connection, Backend::default()));

    send_request(&client, 1, "initialize", json!({"capabilities": {}}));
    let initialized = receive_response(&client);

    assert_eq!(initialized["serverInfo"]["name"], "langmeld");
    assert_eq!(
        initialized["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );

    send_notification(&client, "initialized", Value::Null);
    send_request(&client, 2, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread
        .join()
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}

struct InfoParser;

impl LanguageParser for InfoParser {
    type Tree = ();

    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree> {
        ParsedDocument::new(source, (), Vec::new())
    }
}

#[test]
fn reports_backend_server_info_to_the_client() {
    let (server_connection, client) = Connection::memory();
    let server_thread = thread::spawn(move || {
        let backend = HostLanguageBackend::new(InfoParser)
            .with_server_info(LanguageServerInfo::new("Example Language").with_version("2.4.1"));
        run_connection(server_connection, backend)
    });

    send_request(&client, 1, "initialize", json!({"capabilities": {}}));
    let initialized = receive_response(&client);

    assert_eq!(initialized["serverInfo"]["name"], "Example Language");
    assert_eq!(initialized["serverInfo"]["version"], "2.4.1");

    send_notification(&client, "initialized", Value::Null);
    send_request(&client, 2, "shutdown", Value::Null);
    assert!(receive_response(&client).is_null());
    send_notification(&client, "exit", Value::Null);
    server_thread
        .join()
        .expect("server thread should finish")
        .expect("server should shut down cleanly");
}

#[test]
fn host_backend_accepts_custom_server_info() {
    let expected = LanguageServerInfo::new("Host language").with_version("1.2.3");
    let backend = HostLanguageBackend::new(InfoParser).with_server_info(expected.clone());

    assert_eq!(backend.server_info(), expected);
}

#[test]
fn declarative_backend_accepts_custom_server_info() {
    let language = LanguageSpec::new(
        "Demo",
        [ConstructSpec::new(
            "statement",
            [PatternPart::literal("statement")],
        )],
    )
    .expect("test grammar should be valid");
    let expected = LanguageServerInfo::new("Declarative language").with_version("0.3.0");
    let backend = DeclarativeBackend::new(language).with_server_info(expected.clone());

    assert_eq!(backend.server_info(), expected);
}
