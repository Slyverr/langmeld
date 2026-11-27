//! Scripted editor sessions on Calc, with the whole conversation kept in files, for each
//! way of counting columns. The browser test replays the same files through the web
//! worker, so the two must give the same answers.
//!
//! To write the files again after a change, run with `LANGMELD_WRITE_SESSIONS=1` and read
//! the difference.

use std::path::PathBuf;

use langmeld_ide::{LanguageHooks, Message, Server};
use langmeld_syntax::{Diagnostic, Parse, SourceFile};
use serde_json::{Value, json};

const URI: &str = "file:///work/main.calc";

/// Calc's own check, shown by the editor as the command line shows it.
struct Checks;

impl LanguageHooks for Checks {
    fn diagnostics(&self, file: &SourceFile, parse: &Parse) -> Vec<Diagnostic> {
        if parse.diagnostics.is_empty() {
            calc::checks::divisions_by_zero(file, parse)
        } else {
            Vec::new()
        }
    }
}

/// What the person does, in order. Positions are written in the encoding of the session.
fn script(encoding: &str) -> Vec<Value> {
    // An emoji is two units in UTF-16 and four bytes in UTF-8; `名前` is one unit and
    // three bytes per character.
    let wide = |utf8: u32, utf16: u32| if encoding == "utf-8" { utf8 } else { utf16 };
    let text = "\u{feff}// 😀 greetings\r\nlet 名前 = \"😀\" + 1 / 0;\r\n\
                fn twice(x) {\r\n  x * 2\r\n}\r\nlet total = twice(名前);\r\n";
    let request = |id: u32, method: &str, params: Value| json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    let notification = |method: &str, params: Value| json!({ "jsonrpc": "2.0", "method": method, "params": params });
    let document = json!({ "uri": URI });
    let at = |line: u32, character: u32| json!({ "line": line, "character": character });
    vec![
        request(
            1,
            "initialize",
            json!({ "capabilities": { "general": { "positionEncodings": [encoding] } } }),
        ),
        notification("initialized", json!({})),
        notification(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": URI, "languageId": "calc", "version": 1, "text": text } }),
        ),
        request(
            2,
            "textDocument/documentSymbol",
            json!({ "textDocument": document }),
        ),
        request(
            3,
            "textDocument/foldingRange",
            json!({ "textDocument": document }),
        ),
        request(
            4,
            "textDocument/selectionRange",
            json!({ "textDocument": document, "positions": [at(2, 8)] }),
        ),
        request(
            5,
            "textDocument/semanticTokens/full",
            json!({ "textDocument": document }),
        ),
        // Breaking the call by deleting its `)` on the last line.
        notification(
            "textDocument/didChange",
            json!({ "textDocument": { "uri": URI, "version": 2 },
                    "contentChanges": [{
                        "range": { "start": at(5, wide(24, 20)), "end": at(5, wide(25, 21)) },
                        "text": "" }] }),
        ),
        // A change that arrives late is ignored.
        notification(
            "textDocument/didChange",
            json!({ "textDocument": { "uri": URI, "version": 1 },
                    "contentChanges": [{ "text": "1 +" }] }),
        ),
        // Typing a word in the middle of a statement.
        notification(
            "textDocument/didChange",
            json!({ "textDocument": { "uri": URI, "version": 3 },
                    "contentChanges": [{
                        "range": { "start": at(6, 0), "end": at(6, 0) }, "text": "fn" }] }),
        ),
        request(
            6,
            "textDocument/completion",
            json!({ "textDocument": document, "position": at(6, 2) }),
        ),
        request(7, "shutdown", Value::Null),
        notification("exit", Value::Null),
    ]
}

/// The messages that came back, as JSON the way a client reads them.
fn as_json(replies: Vec<Message>) -> Vec<Value> {
    replies
        .iter()
        .map(|reply| {
            let mut value = serde_json::to_value(reply).expect("JSON");
            value["jsonrpc"] = json!("2.0");
            value
        })
        .collect()
}

fn transcript(encoding: &str) -> Value {
    let loaded = calc::syntax::loaded();
    let mut server = Server::new(loaded, Checks);
    let steps: Vec<Value> = script(encoding)
        .into_iter()
        .map(|sent| {
            let message: Message = serde_json::from_value(sent.clone()).expect("a message");
            json!({ "send": sent, "replies": as_json(server.handle(message)) })
        })
        .collect();
    assert_eq!(server.exit_code(), Some(0));
    Value::Array(steps)
}

fn check(encoding: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/sessions")
        .join(format!("calc-{encoding}.json"));
    let found = format!("{:#}\n", transcript(encoding));
    if std::env::var_os("LANGMELD_WRITE_SESSIONS").is_some() {
        std::fs::write(&path, &found).expect("the file is written");
    }
    let kept = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        kept == found,
        "the session differs from {}: run with LANGMELD_WRITE_SESSIONS=1 to write it again, \
         and read what changed",
        path.display()
    );
}

#[test]
fn the_session_in_utf_8() {
    check("utf-8");
}

#[test]
fn the_session_in_utf_16() {
    check("utf-16");
}
