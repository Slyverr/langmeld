//! Names in the language server: goto, references, rename, completion, hover, quick
//! fixes and symbols of the workspace.

use langmeld_ide::project::MemoryFiles;
use langmeld_ide::{LanguageHooks, Server};
use langmeld_meta::{Loaded, Source, load_language};
use serde_json::{Value, json};

const GRAMMAR: &str = r#"langmeld 1
language Notes { extensions "notes" }
tokens {
    skip ws = /[ \n]+/
    name    = preset ident(ascii)
    text    = preset string(double)
    int     = preset int(dec)
}
rule File   = Item*
rule Item   = Use | Let | Fn | Show
rule Use    = "use" path:text ";"
rule Let    = "let" name:Name "=" value:Value ";"
rule Fn     = "fn" name:Name "{" Item* "}"
rule Show   = "show" value:Value ";"
rule Value  = NameRef | int
rule Name   = name
rule NameRef = name

bind {
    scope File, Fn
    Let.name   defines value after
    Fn.name    defines function hoisted in parent
    NameRef    refers value
    Use        imports value from Use.path
}

editor {
    symbol Let kind constant name name
    symbol Fn  kind function name name
}
"#;

fn language() -> Loaded {
    load_language(Source::text("notes.langmeld", GRAMMAR)).unwrap_or_else(|error| panic!("{error}"))
}

const LIB: &str = "file:///work/lib.notes";
const MAIN: &str = "file:///work/main.notes";

struct Client<'l> {
    server: Server<'l, ()>,
    id: i64,
}

impl<'l> Client<'l> {
    /// A client of a workspace holding `lib.notes`, which it has not opened.
    fn new(language: &'l Loaded) -> Self {
        let mut files = MemoryFiles::default();
        files.insert("/work/lib.notes", "let shared = 1;\nfn helper {}\n");
        let mut client = Self {
            server: Server::new(language, ()).files(files),
            id: 0,
        };
        let result = client.request(
            "initialize",
            json!({ "capabilities": {}, "rootUri": "file:///work" }),
        );
        for provider in [
            "definitionProvider",
            "referencesProvider",
            "hoverProvider",
            "workspaceSymbolProvider",
            "codeActionProvider",
        ] {
            assert_eq!(result["capabilities"][provider], true, "{provider}");
        }
        assert_eq!(
            result["capabilities"]["renameProvider"]["prepareProvider"],
            true
        );
        client
    }

    fn send(&mut self, message: Value) -> Vec<Value> {
        self.server
            .handle_json(&message.to_string())
            .iter()
            .map(|text| serde_json::from_str(text).expect("JSON"))
            .collect()
    }

    fn response(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        let message =
            json!({ "jsonrpc": "2.0", "id": self.id, "method": method, "params": params });
        let mut replies = self.send(message);
        assert_eq!(replies.len(), 1, "{replies:?}");
        replies.remove(0)
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let reply = self.response(method, params);
        assert!(reply.get("error").is_none(), "{reply}");
        reply["result"].clone()
    }

    /// What a change publishes: per document, its codes.
    fn notify(&mut self, method: &str, params: Value) -> Vec<(String, Vec<String>)> {
        let message = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        self.send(message)
            .into_iter()
            .map(|reply| {
                assert_eq!(
                    reply["method"], "textDocument/publishDiagnostics",
                    "{reply}"
                );
                let uri = reply["params"]["uri"].as_str().expect("a uri").to_owned();
                let codes = reply["params"]["diagnostics"]
                    .as_array()
                    .expect("a list")
                    .iter()
                    .map(|d| d["code"].as_str().expect("a code").to_owned())
                    .collect();
                (uri, codes)
            })
            .collect()
    }

    fn open(&mut self, uri: &str, text: &str) -> Vec<(String, Vec<String>)> {
        let document = json!({ "uri": uri, "languageId": "notes", "version": 1, "text": text });
        self.notify("textDocument/didOpen", json!({ "textDocument": document }))
    }

    fn change(&mut self, uri: &str, text: &str) -> Vec<(String, Vec<String>)> {
        self.notify(
            "textDocument/didChange",
            json!({ "textDocument": { "uri": uri, "version": 2 },
                    "contentChanges": [{ "text": text }] }),
        )
    }

    fn at(&mut self, method: &str, uri: &str, line: u32, column: u32, mut more: Value) -> Value {
        more["textDocument"] = json!({ "uri": uri });
        more["position"] = json!({ "line": line, "character": column });
        self.request(method, more)
    }
}

/// `file line:column-column` of a location.
fn location(value: &Value) -> String {
    let uri = value["uri"].as_str().expect("a uri");
    let range = &value["range"];
    format!(
        "{} {}:{}-{}",
        uri.rsplit('/').next().expect("a name"),
        range["start"]["line"],
        range["start"]["character"],
        range["end"]["character"]
    )
}

const PROGRAM: &str = "use \"lib.notes\";\nlet total = shared;\nshow total;\nshow helpr;\n";

#[test]
fn names_cross_the_files_of_the_workspace() {
    let language = language();
    let mut client = Client::new(&language);
    let published = client.open(MAIN, PROGRAM);
    // `helpr` is looked for among values only.
    assert_eq!(published, [(MAIN.to_owned(), vec!["LM-B001".to_owned()])]);

    let found = client.at("textDocument/definition", MAIN, 1, 14, json!({}));
    assert_eq!(location(&found), "lib.notes 0:4-10");

    let found = client.at(
        "textDocument/references",
        MAIN,
        2,
        6,
        json!({ "context": { "includeDeclaration": true } }),
    );
    let found: Vec<_> = found
        .as_array()
        .expect("a list")
        .iter()
        .map(location)
        .collect();
    assert_eq!(found, ["main.notes 1:4-9", "main.notes 2:5-10"]);

    let hover = client.at("textDocument/hover", MAIN, 1, 14, json!({}));
    let text = hover["contents"]["value"].as_str().expect("text");
    assert!(
        text.contains("`shared`") && text.contains("lib.notes"),
        "{text}"
    );
    assert!(text.contains("let shared = 1;"), "{text}");
    assert!(
        client
            .at("textDocument/hover", MAIN, 0, 0, json!({}))
            .is_null()
    );
}

#[test]
fn changing_one_file_updates_the_files_that_see_it() {
    let language = language();
    let mut client = Client::new(&language);
    client.open(MAIN, "use \"lib.notes\";\nshow shared;\n");
    // Opening the library changes nothing seen by `main`.
    let published = client.open(LIB, "let shared = 1;\n");
    assert_eq!(published, [(LIB.to_owned(), vec![])]);
    let published = client.change(LIB, "let other = 1;\n");
    assert_eq!(
        published,
        [
            (LIB.to_owned(), vec![]),
            (MAIN.to_owned(), vec!["LM-B001".to_owned()]),
        ]
    );
    // Closing it goes back to the file on disk.
    let published = client.notify(
        "textDocument/didClose",
        json!({ "textDocument": { "uri": LIB } }),
    );
    assert_eq!(
        published,
        [(LIB.to_owned(), vec![]), (MAIN.to_owned(), vec![])]
    );
}

#[test]
fn renaming_edits_every_file_and_refuses_what_changes_meaning() {
    let language = language();
    let mut client = Client::new(&language);
    client.open(MAIN, PROGRAM);
    let ready = client.at("textDocument/prepareRename", MAIN, 1, 14, json!({}));
    assert_eq!(ready["placeholder"], "shared");
    assert_eq!(ready["range"]["start"]["character"], 12);
    assert!(
        client
            .at("textDocument/prepareRename", MAIN, 0, 0, json!({}))
            .is_null()
    );
    let edit = client.at(
        "textDocument/rename",
        MAIN,
        1,
        14,
        json!({ "newName": "common" }),
    );
    let changes = edit["changes"].as_object().expect("by file");
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[LIB][0]["newText"], "common");
    assert_eq!(changes[MAIN].as_array().map(Vec::len), Some(1));
    client.id += 1;
    let refused = client.response(
        "textDocument/rename",
        json!({ "textDocument": { "uri": MAIN }, "position": { "line": 1, "character": 5 },
                "newName": "helpr" }),
    );
    assert!(
        refused["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("change what they mean")),
        "{refused}"
    );
}

#[test]
fn completion_offers_the_names_in_scope() {
    let language = language();
    let mut client = Client::new(&language);
    client.open(
        MAIN,
        "use \"lib.notes\";\nlet mine = 1;\nshow s;\nfn f {}\n",
    );
    let found = client.at("textDocument/completion", MAIN, 2, 6, json!({}));
    let labels: Vec<_> = found
        .as_array()
        .expect("a list")
        .iter()
        .map(|item| format!("{} {}", item["label"].as_str().expect(""), item["kind"]))
        .collect();
    // Values only, as `show` needs one; no keywords can stand here.
    assert_eq!(labels, ["mine 21", "shared 21"]);
    assert_eq!(found[0]["textEdit"]["range"]["start"]["character"], 5);
    let found = client.at("textDocument/completion", MAIN, 3, 0, json!({}));
    let labels: Vec<_> = found
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|item| item["label"].as_str())
        .collect();
    assert_eq!(labels, ["fn", "let", "show", "use"]);
}

#[test]
fn symbols_of_the_workspace_and_quick_fixes() {
    let language = language();
    let mut client = Client::new(&language);
    client.open(MAIN, "let shade = 1;\nshow shadw;\n");
    let found = client.request("workspace/symbol", json!({ "query": "SHA" }));
    let found: Vec<_> = found
        .as_array()
        .expect("a list")
        .iter()
        .map(|symbol| {
            format!(
                "{} {} {}",
                symbol["name"].as_str().expect("named"),
                symbol["kind"],
                location(&symbol["location"])
            )
        })
        .collect();
    assert_eq!(
        found,
        ["shared 14 lib.notes 0:4-10", "shade 14 main.notes 0:4-9"]
    );
    let actions = client.request(
        "textDocument/codeAction",
        json!({ "textDocument": { "uri": MAIN },
                "range": { "start": { "line": 1, "character": 6 },
                           "end": { "line": 1, "character": 6 } },
                "context": { "diagnostics": [] } }),
    );
    assert_eq!(actions.as_array().map(Vec::len), Some(1), "{actions}");
    assert_eq!(actions[0]["title"], "use `shade`");
    assert_eq!(actions[0]["kind"], "quickfix");
    assert_eq!(actions[0]["edit"]["changes"][MAIN][0]["newText"], "shade");
}

#[test]
fn files_changed_on_disk_are_read_again() {
    let language = language();
    let mut client = Client::new(&language);
    client.open(MAIN, "use \"lib.notes\";\nshow shared;\n");
    let published = client.notify(
        "workspace/didChangeWatchedFiles",
        json!({ "changes": [{ "uri": LIB, "type": 3 }] }),
    );
    assert_eq!(
        published,
        [(
            MAIN.to_owned(),
            vec!["LM-B003".to_owned(), "LM-B001".to_owned()]
        )]
    );
    let published = client.notify(
        "workspace/didChangeWatchedFiles",
        json!({ "changes": [{ "uri": LIB, "type": 1 }] }),
    );
    assert_eq!(published, [(MAIN.to_owned(), vec![])]);
}

struct Hovering;

impl LanguageHooks for Hovering {
    fn hover(
        &self,
        _: &langmeld_ide::bind::Project<'_>,
        _: langmeld_syntax::FileId,
        _: langmeld_syntax::TextSize,
    ) -> Option<String> {
        Some("from the language".to_owned())
    }
}

#[test]
fn the_language_may_describe_names_itself() {
    let language = language();
    let mut server = Server::new(&language, Hovering);
    let mut send = |message: Value| server.handle_json(&message.to_string());
    send(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                 "params": { "capabilities": {} } }));
    send(
        json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
        "textDocument": { "uri": "untitled:1", "languageId": "notes", "version": 1,
                          "text": "let a = 1;" } } }),
    );
    let reply = send(
        json!({ "jsonrpc": "2.0", "id": 2, "method": "textDocument/hover",
        "params": { "textDocument": { "uri": "untitled:1" },
                    "position": { "line": 0, "character": 4 } } }),
    );
    assert!(reply[0].contains("from the language"), "{reply:?}");
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)] // Read, to check the file.
struct App {
    name: String,
    #[serde(default)]
    build: Option<Build>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct Build {
    #[serde(default)]
    jobs: u32,
    #[serde(default)]
    out: String,
}

#[test]
fn project_files_are_checked_and_their_keys_completed() {
    let language = language();
    let mut server = Server::new(&language, ()).project_file::<App>("app.toml");
    let uri = "file:///work/app.toml";
    let mut id = 0;
    let mut send = |method: &str, params: Value| -> Value {
        let message = if method.starts_with("textDocument/did") {
            json!({ "jsonrpc": "2.0", "method": method, "params": params })
        } else {
            id += 1;
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
        };
        let replies = server.handle_json(&message.to_string());
        serde_json::from_str(&replies[0]).expect("JSON")
    };
    send("initialize", json!({ "capabilities": {} }));
    let published = send(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "toml", "version": 1,
                "text": "nmae = \"x\"\n" } }),
    );
    let found = &published["params"]["diagnostics"];
    assert_eq!(found[0]["code"], "LM-M005", "{found}");
    let published = send(
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": 2 },
                "contentChanges": [{ "text": "name = \"x\"\n[build]\njobs = 2\n\n" }] }),
    );
    assert_eq!(published["params"]["diagnostics"], json!([]));
    let mut complete = |line: u32| -> Vec<String> {
        let found = send(
            "textDocument/completion",
            json!({ "textDocument": { "uri": uri },
                    "position": { "line": line, "character": 0 } }),
        );
        let found = found["result"].as_array().expect("a list");
        found
            .iter()
            .filter_map(|item| item["label"].as_str().map(str::to_owned))
            .collect()
    };
    assert_eq!(complete(3), ["out"]);
    // `build` is written as a table below.
    assert!(complete(1).is_empty());
    // Nothing of the language applies to it.
    let symbols = send(
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": uri } }),
    );
    assert!(symbols["result"].is_null(), "{symbols}");
}
