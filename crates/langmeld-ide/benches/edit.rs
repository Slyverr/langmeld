//! The time from an edit to the problems of a long file.

use criterion::{Criterion, criterion_group, criterion_main};
use langmeld_ide::{LanguageHooks, Server};
use langmeld_syntax::{Diagnostic, Parse, SourceFile};
use serde_json::{Value, json};

const URI: &str = "file:///long.calc";

/// Calc's own check, as the command line runs it.
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

fn send(server: &mut Server<'_, Checks>, message: &Value) -> usize {
    server.handle_json(&message.to_string()).len()
}

/// A server with a file of `lines` statements open.
fn opened(lines: usize) -> Server<'static, Checks> {
    let loaded = calc::syntax::loaded();
    let mut server = Server::new(loaded, Checks);
    let initialize = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "capabilities": {} } });
    send(&mut server, &initialize);
    let text: String = (0..lines)
        .map(|n| format!("let value{n} = {n} * (2 + other) - call({n}, \"text\"); // note\n"))
        .collect();
    let open = json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
        "textDocument": { "uri": URI, "languageId": "calc", "version": 1, "text": text } } });
    send(&mut server, &open);
    server
}

fn edit(criterion: &mut Criterion) {
    let mut server = opened(10_000);
    // Typing a character and removing it again; each is a new version of the text.
    let change = |version: u64, start: u32, end: u32, text: &str| {
        json!({ "jsonrpc": "2.0", "method": "textDocument/didChange", "params": {
            "textDocument": { "uri": URI, "version": version },
            "contentChanges": [{ "range": {
                "start": { "line": 5_000, "character": start },
                "end": { "line": 5_000, "character": end } }, "text": text }] } })
    };
    let mut version = 1;
    criterion.bench_function("edit to problems, 10,000 lines", |bencher| {
        let mut present = false;
        bencher.iter(|| {
            present = !present;
            version += 1;
            let message = if present {
                change(version, 4, 4, "x")
            } else {
                change(version, 4, 5, "")
            };
            send(&mut server, &message)
        });
    });
    let tokens = json!({ "jsonrpc": "2.0", "id": 2, "method": "textDocument/semanticTokens/full",
        "params": { "textDocument": { "uri": URI } } });
    criterion.bench_function("colours, 10,000 lines", |bencher| {
        bencher.iter(|| send(&mut server, &tokens));
    });
}

criterion_group!(benches, edit);
criterion_main!(benches);
