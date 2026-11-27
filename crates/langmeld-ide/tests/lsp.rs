//! The language server, driven message by message.

use langmeld_ide::{LanguageHooks, Server};
use langmeld_meta::{Loaded, Source, load_language};
use langmeld_syntax::{Diagnostic, Parse, SourceFile, TextRange, TextSize};
use serde_json::{Value, json};

const URI: &str = "file:///work/main.shapes";

const SHAPES: &str = r#"langmeld 1
language Shapes {
    extensions "shapes"
    comments { line "//"  block "/*" "*/" }
}
tokens {
    skip space = /[ \t\r\n]+/
    word       = preset ident(xid)
    int        = preset int(dec)
    text       = preset string(double)
    contextual { from }
}
rule File  = Item*
rule Item  = Group | Let
rule Group = "group" name:Name Body
rule Body  = "{" Item* "}"
rule Let   = "let" name:Name "=" sep(Value, ",") ";"
rule Name  = word
expr Value {
    atom Number = int
    atom Quoted = text
    atom Use    = word
    infix Sum "+" prec 1 left
}
editor {
    symbol Group kind module name name
    symbol Let   kind variable name name
    fold   Body
}
"#;

fn shapes() -> Loaded {
    load_language(Source::text("shapes.langmeld", SHAPES)).unwrap_or_else(|error| panic!("{error}"))
}

struct Client<'l, H: LanguageHooks> {
    server: Server<'l, H>,
    id: i64,
}

impl<'l, H: LanguageHooks> Client<'l, H> {
    fn new(loaded: &'l Loaded, hooks: H, encodings: &[&str]) -> (Self, Value) {
        let server = Server::new(loaded, hooks);
        Self::over(server, encodings)
    }

    fn over(server: Server<'l, H>, encodings: &[&str]) -> (Self, Value) {
        let mut client = Self { server, id: 0 };
        let capabilities = json!({ "general": { "positionEncodings": encodings } });
        let result = client.request("initialize", json!({ "capabilities": capabilities }));
        assert!(client.notify("initialized", json!({})).is_empty());
        (client, result)
    }

    fn send(&mut self, message: Value) -> Vec<Value> {
        self.server
            .handle_json(&message.to_string())
            .iter()
            .map(|text| serde_json::from_str(text).expect("JSON"))
            .collect()
    }

    /// The whole response to a request, with the messages that came with it.
    fn replies(&mut self, method: &str, params: Value) -> Vec<Value> {
        self.id += 1;
        let id = self.id;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
    }

    fn response(&mut self, method: &str, params: Value) -> Value {
        let id = self.id + 1;
        let mut replies = self.replies(method, params);
        assert_eq!(replies.len(), 1, "{replies:?}");
        let reply = replies.remove(0);
        assert_eq!(reply["id"], id);
        assert_eq!(reply["jsonrpc"], "2.0");
        reply
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let reply = self.response(method, params);
        assert!(reply.get("error").is_none(), "{reply}");
        reply["result"].clone()
    }

    fn notify(&mut self, method: &str, params: Value) -> Vec<Value> {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }))
    }

    /// Opens a document and gives its diagnostics.
    fn open(&mut self, text: &str) -> Vec<Value> {
        let document = json!({ "uri": URI, "languageId": "shapes", "version": 1, "text": text });
        published(self.notify("textDocument/didOpen", json!({ "textDocument": document })))
    }

    fn change(&mut self, version: i64, changes: Value) -> Vec<Value> {
        let document = json!({ "uri": URI, "version": version });
        published(self.notify(
            "textDocument/didChange",
            json!({ "textDocument": document, "contentChanges": changes }),
        ))
    }

    fn about(&mut self, method: &str, mut params: Value) -> Value {
        params["textDocument"] = json!({ "uri": URI });
        self.request(method, params)
    }
}

fn published(messages: Vec<Value>) -> Vec<Value> {
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert_eq!(messages[0]["method"], "textDocument/publishDiagnostics");
    messages[0]["params"]["diagnostics"]
        .as_array()
        .expect("a list")
        .clone()
}

/// `line:character-line:character` of a range.
fn range(value: &Value) -> String {
    let at = |end: &str| {
        format!(
            "{}:{}",
            value[end]["line"].as_u64().expect("a line"),
            value[end]["character"].as_u64().expect("a column")
        )
    };
    format!("{}-{}", at("start"), at("end"))
}

fn position(line: u32, character: u32) -> Value {
    json!({ "line": line, "character": character })
}

#[test]
fn it_says_what_it_can_do() {
    let loaded = shapes();
    let (_, result) = Client::new(&loaded, (), &["utf-16", "utf-8"]);
    let capabilities = &result["capabilities"];
    assert_eq!(capabilities["positionEncoding"], "utf-8");
    assert_eq!(capabilities["textDocumentSync"], 2);
    for provider in [
        "documentSymbolProvider",
        "foldingRangeProvider",
        "selectionRangeProvider",
        "semanticTokensProvider",
        "completionProvider",
    ] {
        assert!(!capabilities[provider].is_null(), "{provider}");
    }
    assert!(capabilities["definitionProvider"].is_null());
    assert_eq!(result["serverInfo"]["name"], "Shapes language server");

    let (_, result) = Client::new(&loaded, (), &[]);
    assert_eq!(result["capabilities"]["positionEncoding"], "utf-16");
}

#[test]
fn problems_are_published_in_the_negotiated_encoding() {
    let loaded = shapes();
    // The emoji takes four bytes, two UTF-16 units, and the lines end with CRLF.
    let text = "let a = \"😀\";\r\nlet b = \"😀\" 1;\r\n";
    for (encodings, wanted) in [
        (&["utf-8"][..], "1:15-1:16"),
        (&["utf-16"][..], "1:13-1:14"),
    ] {
        let (mut client, _) = Client::new(&loaded, (), encodings);
        let found = client.open(text);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(range(&found[0]["range"]), wanted, "{encodings:?}");
        assert_eq!(found[0]["code"], "LM-P001");
        assert_eq!(found[0]["severity"], 1);
        assert_eq!(found[0]["source"], "shapes");
        assert_eq!(found[0]["version"], Value::Null);
    }
}

#[test]
fn a_byte_order_mark_and_wide_characters_keep_positions_right() {
    let loaded = shapes();
    // A mark (3 bytes, 1 unit), CJK (3 bytes, 1 unit) and an emoji (4 bytes, 2 units).
    let text = "\u{feff}let 名前 = \"😀\" 1;\n";
    for (encodings, wanted) in [
        (&["utf-8"][..], "0:23-0:24"),
        (&["utf-16"][..], "0:15-0:16"),
    ] {
        let (mut client, _) = Client::new(&loaded, (), encodings);
        let found = client.open(text);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(range(&found[0]["range"]), wanted, "{encodings:?}");
    }
}

#[test]
fn changes_are_applied_where_they_are_made() {
    let loaded = shapes();
    let (mut client, _) = Client::new(&loaded, (), &["utf-16"]);
    assert_eq!(client.open("let a = 1;\r\nlet 😀 = 2;\r\n").len(), 1);
    // Replace the emoji, which is two units long, with a name.
    let found = client.change(
        2,
        json!([{ "range": { "start": position(1, 4), "end": position(1, 6) }, "text": "b" }]),
    );
    assert_eq!(found, Vec::<Value>::new());
    // Several changes in one message apply one after another.
    let found = client.change(
        3,
        json!([
            { "range": { "start": position(0, 0), "end": position(0, 0) }, "text": "\n" },
            { "range": { "start": position(1, 4), "end": position(1, 5) }, "text": "" },
        ]),
    );
    assert_eq!(found.len(), 1);
    assert_eq!(range(&found[0]["range"]), "1:5-1:6");
    // A whole new text, and positions past the end.
    assert!(
        client
            .change(4, json!([{ "text": "let c = 3;" }]))
            .is_empty()
    );
    let past =
        json!([{ "range": { "start": position(9, 9), "end": position(99, 0) }, "text": " x" }]);
    assert_eq!(client.change(5, past).len(), 1);
    let closed = published(client.notify(
        "textDocument/didClose",
        json!({ "textDocument": { "uri": URI } }),
    ));
    assert!(closed.is_empty());
    assert!(
        client
            .about("textDocument/documentSymbol", json!({}))
            .is_null()
    );
}

#[test]
fn a_change_that_splits_a_character_does_not_panic() {
    let loaded = shapes();
    for encoding in ["utf-8", "utf-16"] {
        let (mut client, _) = Client::new(&loaded, (), &[encoding]);
        client.open("let 😀 = 名;\n");
        // Columns inside a character mean the start of it, in either encoding.
        for column in 0..14 {
            let edit = json!([{
                "range": { "start": position(0, column), "end": position(0, column + 1) },
                "text": "é",
            }]);
            client.change(2 + i64::from(column), edit);
        }
        client.about("textDocument/semanticTokens/full", json!({}));
        client.about(
            "textDocument/completion",
            json!({ "position": position(0, 5) }),
        );
    }
}

#[test]
fn stale_changes_are_ignored() {
    let loaded = shapes();
    let (mut client, _) = Client::new(&loaded, (), &["utf-8"]);
    client.open("let a = 1;");
    let found = client.change(3, json!([{ "text": "let a = 1 2;" }]));
    assert_eq!(found.len(), 1);
    let document = json!({ "uri": URI, "version": 2 });
    // Version 2 comes after version 3: it is out of date, and says nothing about the text.
    let replies = client.notify(
        "textDocument/didChange",
        json!({ "textDocument": document, "contentChanges": [{ "text": "let a = 1;" }] }),
    );
    assert_eq!(replies.len(), 1, "{replies:?}");
    assert_eq!(replies[0]["method"], "window/logMessage");
    assert_eq!(
        replies[0]["params"]["message"],
        "ignored a change of `file:///work/main.shapes` to version 2: it is at version 3"
    );
    // The text is still the one of version 3, and its problems are published with 3.
    let symbols = client.about("textDocument/semanticTokens/full", json!({}));
    assert!(
        symbols["data"]
            .as_array()
            .is_some_and(|data| !data.is_empty())
    );
    let replies = client.notify(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": URI, "version": 4 },
                "contentChanges": [{ "text": "let a = 1 2 3;" }] }),
    );
    assert_eq!(replies[0]["params"]["version"], 4);
    assert_eq!(replies[0]["params"]["diagnostics"][0]["code"], "LM-P001");
}

const PROGRAM: &str = "group shapes {\n  let size = 1 + 2;\n  group inner {\n  }\n}\n\
                       /* a\n   comment */\nlet other = size;\n";

#[test]
fn symbols_form_an_outline() {
    let loaded = shapes();
    let (mut client, _) = Client::new(&loaded, (), &["utf-8"]);
    client.open(PROGRAM);
    let symbols = client.about("textDocument/documentSymbol", json!({}));
    fn outline(symbols: &Value, depth: usize, into: &mut Vec<String>) {
        for symbol in symbols.as_array().into_iter().flatten() {
            into.push(format!(
                "{}{} {} {}",
                "  ".repeat(depth),
                symbol["name"].as_str().expect("named"),
                symbol["kind"],
                range(&symbol["selectionRange"])
            ));
            outline(&symbol["children"], depth + 1, into);
        }
    }
    let mut found = Vec::new();
    outline(&symbols, 0, &mut found);
    assert_eq!(
        found,
        [
            "shapes 2 0:6-0:12",
            "  size 13 1:6-1:10",
            "  inner 2 2:8-2:13",
            "other 13 7:4-7:9",
        ]
    );
}

#[test]
fn blocks_and_comments_fold() {
    let loaded = shapes();
    let (mut client, _) = Client::new(&loaded, (), &["utf-8"]);
    client.open(PROGRAM);
    let folds = client.about("textDocument/foldingRange", json!({}));
    let found: Vec<_> = folds
        .as_array()
        .expect("a list")
        .iter()
        .map(|fold| {
            let kind = fold["kind"].as_str().unwrap_or("region");
            format!("{}-{} {kind}", fold["startLine"], fold["endLine"])
        })
        .collect();
    // The closing brace stays in sight.
    assert_eq!(found, ["0-3 region", "5-6 comment"]);
}

#[test]
fn selection_widens_node_by_node() {
    let loaded = shapes();
    let (mut client, _) = Client::new(&loaded, (), &["utf-8"]);
    client.open(PROGRAM);
    let result = client.about(
        "textDocument/selectionRange",
        json!({ "positions": [position(1, 15)] }),
    );
    let mut next = result[0].clone();
    let mut found = Vec::new();
    while !next.is_null() {
        found.push(range(&next["range"]));
        next = next["parent"].clone();
    }
    assert_eq!(
        found,
        [
            "1:15-1:16",
            "1:13-1:18",
            "1:2-1:19",
            "0:13-4:1",
            "0:0-4:1",
            "0:0-8:0",
        ]
    );
}

#[test]
fn tokens_are_coloured() {
    let loaded = shapes();
    let (mut client, result) = Client::new(&loaded, (), &["utf-16"]);
    let legend = &result["capabilities"]["semanticTokensProvider"]["legend"]["tokenTypes"];
    let types: Vec<String> = serde_json::from_value(legend.clone()).expect("names");
    client.open("// 😀\nlet a = \"x\" + b; /* 1\n2 */\n");
    let data = client.about("textDocument/semanticTokens/full", json!({}))["data"].clone();
    let numbers: Vec<u64> = serde_json::from_value(data).expect("numbers");
    let (mut line, mut start) = (0, 0);
    let mut found = Vec::new();
    for token in numbers.chunks(5) {
        line += token[0];
        start = if token[0] == 0 {
            start + token[1]
        } else {
            token[1]
        };
        let kind = &types[usize::try_from(token[3]).expect("small")];
        found.push(format!("{line}:{start}+{} {kind}", token[2]));
    }
    assert_eq!(
        found,
        [
            "0:0+5 comment",
            "1:0+3 keyword",
            "1:4+1 variable",
            "1:6+1 operator",
            "1:8+3 string",
            "1:12+1 operator",
            "1:14+1 variable",
            "1:17+4 comment",
            "2:0+4 comment",
        ]
    );
}

#[test]
fn keywords_complete_the_word_being_written() {
    let loaded = shapes();
    let (mut client, _) = Client::new(&loaded, (), &["utf-8"]);
    client.open("group a {\n  le\n}\n");
    let result = client.about(
        "textDocument/completion",
        json!({ "position": position(1, 3) }),
    );
    let items = result.as_array().expect("a list");
    let labels: Vec<_> = items.iter().filter_map(|i| i["label"].as_str()).collect();
    assert_eq!(labels, ["group", "let"]);
    assert_eq!(range(&items[0]["textEdit"]["range"]), "1:2-1:4");
    assert_eq!(items[0]["textEdit"]["newText"], "group");
    assert_eq!(items[0]["kind"], 14);
    // In the middle of a word the whole word is replaced.
    let result = client.about(
        "textDocument/completion",
        json!({ "position": position(1, 2) }),
    );
    assert_eq!(range(&result[0]["textEdit"]["range"]), "1:2-1:4");
    // Where a name is expected, no keyword is offered.
    client.change(2, json!([{ "text": "let " }]));
    let result = client.about(
        "textDocument/completion",
        json!({ "position": position(0, 4) }),
    );
    assert_eq!(result, json!([]));
}

struct Checks;

impl LanguageHooks for Checks {
    fn diagnostics(&self, file: &SourceFile, parse: &Parse) -> Vec<Diagnostic> {
        assert!(!file.text().contains("panic"), "asked to panic");
        // The parser's own problems are not listed twice.
        let mut found = parse.diagnostics.clone();
        if let Some(at) = file.text().find("bad") {
            let at = TextSize::new(u32::try_from(at).expect("small"));
            let range = TextRange::new(at, at + TextSize::new(3));
            let first = TextRange::empty(TextSize::new(0));
            found.push(
                Diagnostic::warning("SHAPES001", file.id(), range, "`bad` is a bad name")
                    .label(langmeld_syntax::Label::new(
                        file.id(),
                        first,
                        "the file starts here",
                    ))
                    .note("choose another"),
            );
        }
        found
    }

    fn completions(&self, file: &SourceFile, _: &Parse, _: TextSize) -> Vec<String> {
        assert!(!file.text().contains("crash"), "asked to crash");
        vec!["square".to_owned()]
    }
}

#[test]
fn the_language_adds_its_own_checks_and_words() {
    let loaded = shapes();
    let (mut client, _) = Client::new(&loaded, Checks, &["utf-8"]);
    let found = client.open("let bad = 1 2;");
    let codes: Vec<_> = found.iter().filter_map(|d| d["code"].as_str()).collect();
    assert_eq!(codes, ["SHAPES001", "LM-P001"]);
    assert_eq!(found[0]["severity"], 2);
    assert_eq!(found[0]["message"], "`bad` is a bad name\nchoose another");
    assert_eq!(
        found[0]["relatedInformation"][0]["message"],
        "the file starts here"
    );
    let items = client.about(
        "textDocument/completion",
        json!({ "position": position(0, 14) }),
    );
    let labels: Vec<_> = items
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|i| i["label"].as_str())
        .collect();
    assert!(labels.contains(&"square"), "{labels:?}");
}

#[test]
fn a_failure_in_a_check_is_logged_and_the_server_goes_on() {
    let loaded = shapes();
    let (mut client, _) = Client::new(&loaded, Checks, &["utf-8"]);
    let replies = client.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": URI, "languageId": "shapes", "version": 1,
                                  "text": "let panic = 1;\nlet crash = 1 2;" } }),
    );
    // The log, then the problems of the parser, which are still shown.
    assert_eq!(replies.len(), 2, "{replies:?}");
    assert_eq!(replies[0]["method"], "window/logMessage");
    assert_eq!(
        replies[0]["params"]["message"],
        "the language's checks failed: asked to panic"
    );
    assert_eq!(replies[1]["params"]["diagnostics"][0]["code"], "LM-P001");
    // Other features work on the same document.
    client.about("textDocument/semanticTokens/full", json!({}));
}

#[test]
fn a_failure_in_a_request_logs_and_answers_with_nothing() {
    let loaded = shapes();
    let (mut client, _) = Client::new(&loaded, Checks, &["utf-8"]);
    client.open("let crash = 1;");
    let params = json!({ "textDocument": { "uri": URI }, "position": position(0, 5) });
    let replies = client.replies("textDocument/completion", params);
    assert_eq!(replies.len(), 2, "{replies:?}");
    assert_eq!(replies[0]["method"], "window/logMessage");
    assert_eq!(
        replies[0]["params"]["message"],
        "`textDocument/completion` failed: asked to crash"
    );
    assert_eq!(replies[1]["result"], Value::Null);
    assert!(replies[1].get("error").is_none());
    // The next request is answered as usual.
    let symbols = client.about("textDocument/documentSymbol", json!({}));
    assert_eq!(symbols[0]["name"], "crash");
}

#[test]
fn a_document_over_the_limit_is_one_problem() {
    let loaded = shapes();
    let server = Server::new(&loaded, ()).limit(16);
    let (mut client, _) = Client::over(server, &["utf-8"]);
    let found = client.open("let a = 1; let b = 2;");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["code"], "LM-E001");
    assert_eq!(
        found[0]["message"],
        "the file is too large for the language server: it has 21 bytes, and the most is 16, \
         so no problems are shown for it\nTo fix this, split the file into smaller ones."
    );
    assert_eq!(range(&found[0]["range"]), "0:0-0:0");
    assert!(
        client
            .about("textDocument/documentSymbol", json!({}))
            .is_null()
    );
    // Making it smaller brings the features back.
    let found = client.change(2, json!([{ "text": "let a = 1 2;" }]));
    assert_eq!(found[0]["code"], "LM-P001");
    assert_eq!(
        client.about("textDocument/documentSymbol", json!({}))[0]["name"],
        "a"
    );
}

#[test]
fn it_follows_the_lifecycle() {
    let loaded = shapes();
    let mut server = Server::new(&loaded, ());
    let request = |server: &mut Server<'_, ()>, id: u32, method: &str, params: Value| {
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let replies = server.handle_json(&message.to_string());
        serde_json::from_str::<Value>(&replies[0]).expect("JSON")
    };
    let early = request(&mut server, 1, "textDocument/documentSymbol", json!({}));
    assert_eq!(early["error"]["code"], -32002);
    let init = json!({ "capabilities": {} });
    assert!(request(&mut server, 2, "initialize", init.clone())["error"].is_null());
    assert_eq!(
        request(&mut server, 3, "initialize", init)["error"]["code"],
        -32600
    );
    let unknown = request(&mut server, 4, "textDocument/nothing", json!({}));
    assert_eq!(unknown["error"]["code"], -32601);
    let broken = request(&mut server, 5, "textDocument/completion", json!({ "x": 1 }));
    assert_eq!(broken["error"]["code"], -32602);
    assert_eq!(
        request(&mut server, 6, "shutdown", Value::Null)["result"],
        Value::Null
    );
    let late = request(&mut server, 7, "textDocument/documentSymbol", json!({}));
    assert_eq!(late["error"]["code"], -32600);
    assert_eq!(server.exit_code(), None);
    assert!(
        server
            .handle_json(r#"{"jsonrpc":"2.0","method":"exit"}"#)
            .is_empty()
    );
    assert_eq!(server.exit_code(), Some(0));

    let mut abrupt = Server::new(&loaded, ());
    abrupt.handle_json(r#"{"jsonrpc":"2.0","method":"exit"}"#);
    assert_eq!(abrupt.exit_code(), Some(1));
    let garbage = Server::new(&loaded, ()).handle_json("not JSON");
    assert_eq!(garbage.len(), 1);
    assert!(garbage[0].contains("-32700"), "{garbage:?}");
}

struct Definitions;

impl LanguageHooks for Definitions {
    fn provides_definitions(&self) -> bool {
        true
    }

    fn definition(&self, _: &SourceFile, _: &Parse, offset: TextSize) -> Option<TextRange> {
        (offset > TextSize::new(0)).then(|| TextRange::new(TextSize::new(4), TextSize::new(5)))
    }
}

#[test]
fn a_language_can_lead_a_name_to_its_declaration() {
    let loaded = shapes();
    let (mut client, result) = Client::new(&loaded, Definitions, &["utf-8"]);
    assert_eq!(result["capabilities"]["definitionProvider"], true);
    client.open("let a = 1;");
    let found = client.about(
        "textDocument/definition",
        json!({ "position": position(0, 6) }),
    );
    assert_eq!(found["uri"], URI);
    assert_eq!(range(&found["range"]), "0:4-0:5");
    let nothing = client.about(
        "textDocument/definition",
        json!({ "position": position(0, 0) }),
    );
    assert!(nothing.is_null());
}
