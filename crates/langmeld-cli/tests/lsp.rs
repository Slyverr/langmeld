//! `langmeld lsp`, spoken to as an editor does.

mod support;

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn framed(message: Value) -> String {
    let text = message.to_string();
    format!("Content-Length: {}\r\n\r\n{text}", text.len())
}

/// Starts a server, sends it the messages and gives its replies and exit code.
fn session(command: &mut Command, messages: Vec<Value>) -> (Vec<Value>, i32) {
    let mut server = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the server starts");
    let mut input = server.stdin.take().expect("piped");
    for message in messages {
        input
            .write_all(framed(message).as_bytes())
            .expect("the server reads");
    }
    drop(input);
    let mut output = String::new();
    server
        .stdout
        .take()
        .expect("piped")
        .read_to_string(&mut output)
        .expect("UTF-8");
    let code = server.wait().expect("ends").code().expect("exits");
    let replies = output
        .split("Content-Length: ")
        .filter_map(|part| part.split_once("\r\n\r\n"))
        .map(|(_, body)| serde_json::from_str(body).expect("JSON"))
        .collect();
    (replies, code)
}

#[test]
fn grammar_files_get_problems_and_definitions() {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("lsp-grammar");
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("language")).expect("created");
    std::fs::write(
        folder.join("language/words.langmeld"),
        "langmeld 1\ntokens { skip s = / +/ name = preset ident(ascii) }\nrule Word = name\n",
    )
    .expect("written");
    let main = "langmeld 1\nlanguage Words { extensions \"w\" }\nimport \"words.langmeld\"\n\
                rule File = Word* Nope?\n";
    std::fs::write(folder.join("language/main.langmeld"), "unsaved").expect("written");
    let folder = folder.canonicalize().expect("there");
    // The editor has text that is not saved yet.
    let uri = |name: &str| format!("file://{}/language/{name}", folder.display());
    let open = |name: &str, text: &str| {
        json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
            "textDocument": { "uri": uri(name), "languageId": "langmeld", "version": 1,
                              "text": text } } })
    };
    let messages = vec![
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "capabilities": {} } }),
        json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
        open("main.langmeld", main),
        open("words.langmeld", "langmeld 1\nrule Word = missing\n"),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "textDocument/definition", "params": {
            "textDocument": { "uri": uri("main.langmeld") },
            "position": { "line": 3, "character": 6 } } }),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "textDocument/definition", "params": {
            "textDocument": { "uri": uri("words.langmeld") },
            "position": { "line": 1, "character": 6 } } }),
        json!({ "jsonrpc": "2.0", "id": 4, "method": "shutdown" }),
        json!({ "jsonrpc": "2.0", "method": "exit" }),
    ];
    let mut command = Command::new(env!("CARGO_BIN_EXE_langmeld"));
    // Editors often say which transport they use.
    let (replies, code) = session(command.args(["lsp", "--stdio"]), messages);
    assert_eq!(code, 0);
    assert_eq!(replies.len(), 6, "{replies:#?}");
    assert_eq!(
        replies[0]["result"]["capabilities"]["definitionProvider"],
        true
    );

    let found = &replies[1]["params"]["diagnostics"];
    assert_eq!(found.as_array().map(Vec::len), Some(1), "{found}");
    assert_eq!(found[0]["code"], "LM-G001");
    assert_eq!(
        found[0]["range"]["start"],
        json!({ "line": 3, "character": 18 })
    );

    // An imported file is checked as part of its project.
    let found = &replies[2]["params"]["diagnostics"];
    assert_eq!(found.as_array().map(Vec::len), Some(1), "{found}");
    assert!(
        found[0]["message"]
            .as_str()
            .is_some_and(|m| m.contains("missing")),
        "{found}"
    );
    // A rule leads to its declaration, here the rule itself.
    assert_eq!(
        replies[3]["result"]["range"]["start"],
        json!({ "line": 3, "character": 5 }),
        "{replies:#?}"
    );
    // Where the name of a rule is written in its own declaration, it leads to itself.
    assert_eq!(
        replies[4]["result"]["range"]["start"],
        json!({ "line": 1, "character": 5 }),
        "{replies:#?}"
    );
}

#[test]
fn a_message_that_cannot_be_read_is_logged_and_the_next_is_answered() {
    let mut server = Command::new(env!("CARGO_BIN_EXE_langmeld"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the server starts");
    let mut input = server.stdin.take().expect("piped");
    // A frame whose body is not JSON, then a request.
    input
        .write_all(b"Content-Length: 5\r\n\r\nhello")
        .expect("the server reads");
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                          "params": { "capabilities": {} } });
    input
        .write_all(framed(request).as_bytes())
        .expect("the server reads");
    input
        .write_all(framed(json!({ "jsonrpc": "2.0", "method": "exit" })).as_bytes())
        .expect("the server reads");
    drop(input);
    let mut output = String::new();
    server
        .stdout
        .take()
        .expect("piped")
        .read_to_string(&mut output)
        .expect("UTF-8");
    // `exit` without a shutdown is the code of a client that went away.
    assert_eq!(server.wait().expect("ends").code(), Some(1));
    assert!(output.contains("a message could not be read"), "{output}");
    assert!(output.contains("\"capabilities\""), "{output}");
}
