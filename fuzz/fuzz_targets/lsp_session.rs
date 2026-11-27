//! Requests and edits of any kind, in any order, to the language server of Calc and to
//! the one of `.langmeld` files: the server answers each message, never loses its
//! mind (a failure inside it is logged, and counts as a bug here), and only publishes
//! problems of the text it holds.

#![no_main]

use langmeld_ide::{LanguageHooks, Message, Served, Server};
use langmeld_meta::{grammar_editor, grammar_language};
use libfuzzer_sys::fuzz_target;
use serde_json::{Value, json};

/// Pieces of text, so that random bytes make programs and edits that matter.
const PIECES: &[&str] = &[
    "let ",
    "fn ",
    "if ",
    "else ",
    "x",
    "1",
    "\"s\"",
    "(",
    ")",
    "{",
    "}",
    ",",
    ";",
    "=",
    "+",
    "/",
    "0",
    " ",
    "\n",
    "\r\n",
    "//",
    "/*",
    "*/",
    "😀",
    "名",
    "\u{feff}",
    "\u{0}",
    "rule ",
    "expr ",
    "language ",
    "tokens ",
    "editor ",
    "symbol ",
    "fold ",
    "import ",
    "\"",
    "@",
];

/// The bytes left to read, which run out into zeros.
struct Bytes<'a>(&'a [u8]);

impl Bytes<'_> {
    fn u8(&mut self) -> u8 {
        match self.0.split_first() {
            Some((&byte, rest)) => {
                self.0 = rest;
                byte
            }
            None => 0,
        }
    }

    /// A number that is mostly small, and sometimes very large.
    fn number(&mut self) -> u32 {
        let small = u32::from(self.u8() % 12);
        match self.u8() % 8 {
            0 => u32::MAX - small,
            1 => u32::from(self.u8()) << 8 | u32::from(self.u8()),
            _ => small,
        }
    }

    fn text(&mut self) -> String {
        let count = self.u8() % 6;
        (0..count)
            .map(|_| PIECES[usize::from(self.u8()) % PIECES.len()])
            .collect()
    }

    fn done(&self) -> bool {
        self.0.is_empty()
    }
}

/// Calc's own check, as the command line runs it.
struct Checks;

impl LanguageHooks for Checks {
    fn diagnostics(
        &self,
        file: &langmeld_syntax::SourceFile,
        parse: &langmeld_syntax::Parse,
    ) -> Vec<langmeld_syntax::Diagnostic> {
        if parse.diagnostics.is_empty() {
            calc::checks::divisions_by_zero(file, parse)
        } else {
            Vec::new()
        }
    }
}

fn message(value: Value) -> Message {
    serde_json::from_value(value).expect("a message of the protocol")
}

fn position(bytes: &mut Bytes<'_>) -> Value {
    json!({ "line": bytes.number(), "character": bytes.number() })
}

/// Sends the messages the bytes describe to `server`, and checks the answers.
fn session<H: LanguageHooks>(server: &mut Server<'_, H>, data: &[u8]) {
    let mut bytes = Bytes(data);
    let encoding = if bytes.u8() % 2 == 0 {
        "utf-8"
    } else {
        "utf-16"
    };
    let mut id = 0;
    let send = |server: &mut Server<'_, H>, value: Value| {
        for reply in server.handle(message(value)) {
            let shown = serde_json::to_value(&reply).expect("JSON");
            // A failure inside the server is logged and answered with nothing; none may
            // happen here.
            if let Some(text) = shown["params"]["message"].as_str() {
                assert!(!text.contains(" failed: "), "{text}");
            }
            for found in shown["params"]["diagnostics"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let (start, end) = (&found["range"]["start"], &found["range"]["end"]);
                assert!(
                    (start["line"].as_u64(), start["character"].as_u64())
                        <= (end["line"].as_u64(), end["character"].as_u64()),
                    "{found}"
                );
            }
        }
    };
    id += 1;
    send(
        server,
        json!({ "jsonrpc": "2.0", "id": id, "method": "initialize", "params": {
            "capabilities": { "general": { "positionEncodings": [encoding] } } } }),
    );
    let uri = |n: u8| format!("file:///fuzz/{n}.calc");
    let mut version = [0u32; 4];
    while !bytes.done() {
        let document = bytes.u8() % 4;
        let at = uri(document);
        let slot = &mut version[usize::from(document)];
        match bytes.u8() % 10 {
            0 => {
                *slot = 1 + u32::from(bytes.u8() % 3);
                let text = bytes.text();
                send(
                    server,
                    json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
                        "textDocument": { "uri": at, "languageId": "x", "version": *slot,
                                          "text": text } } }),
                );
            }
            1..=3 => {
                // Mostly newer versions, sometimes old ones.
                *slot = if bytes.u8() % 5 == 0 {
                    slot.saturating_sub(1)
                } else {
                    slot.saturating_add(1)
                };
                let change = if bytes.u8() % 4 == 0 {
                    json!({ "text": bytes.text() })
                } else {
                    json!({ "range": { "start": position(&mut bytes),
                                       "end": position(&mut bytes) },
                             "text": bytes.text() })
                };
                let mut changes = vec![change];
                if bytes.u8() % 2 == 0 {
                    changes.push(json!({ "text": bytes.text() }));
                }
                send(
                    server,
                    json!({ "jsonrpc": "2.0", "method": "textDocument/didChange", "params": {
                        "textDocument": { "uri": at, "version": *slot },
                        "contentChanges": changes } }),
                );
            }
            4 => {
                send(
                    server,
                    json!({ "jsonrpc": "2.0", "method": "textDocument/didClose", "params": {
                        "textDocument": { "uri": at } } }),
                );
            }
            kind => {
                id += 1;
                let td = json!({ "uri": at });
                let (method, params) = match kind {
                    5 => (
                        "textDocument/completion",
                        json!({ "textDocument": td,
                                "position": position(&mut bytes) }),
                    ),
                    6 => ("textDocument/documentSymbol", json!({ "textDocument": td })),
                    7 => ("textDocument/foldingRange", json!({ "textDocument": td })),
                    8 => (
                        "textDocument/semanticTokens/full",
                        json!({ "textDocument": td }),
                    ),
                    _ => (
                        "textDocument/selectionRange",
                        json!({ "textDocument": td,
                                "positions": [position(&mut bytes), position(&mut bytes)] }),
                    ),
                };
                send(
                    server,
                    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
                );
            }
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let loaded = calc::syntax::loaded();
    let mut calc = Server::new(loaded, Checks).limit(1 << 16);
    session(&mut calc, data);
    let mut grammar =
        Server::new(Served::new(grammar_language()).editor(grammar_editor()), ()).limit(1 << 16);
    session(&mut grammar, data);
});
