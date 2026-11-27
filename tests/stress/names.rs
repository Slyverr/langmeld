//! The names of the stress-test language: global pipelines across files, a stage that
//! sees the tasks of the pipeline it is for, names a repetition makes, and a name used
//! in the hole of a text. Each has goto, references and rename, in the library and in
//! a language server.

#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;
use std::sync::OnceLock;

use langmeld::bind::{Bindings, Project, RenameError, Resolver};
use langmeld::ide::{LanguageHooks, Server};
use langmeld::load_language;
use langmeld::project::MemoryFiles;
use langmeld::{FileId, Language, Loaded, Source, SourceFile, SyntaxNode, TextRange, TextSize};
use serde_json::{Value, json};

fn loaded() -> &'static Loaded {
    static LOADED: OnceLock<Loaded> = OnceLock::new();
    LOADED.get_or_init(|| {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR"));
        load_language(Source::path(folder.join("pipes.langmeld")))
            .unwrap_or_else(|error| panic!("{error}"))
    })
}

/// The most names one task may stand for.
const MOST: u32 = 1_000;

/// The first and last number of a range such as `1..=3` or `1..4`.
fn bounds(text: &str) -> Option<(u32, u32)> {
    let text = text.trim();
    if let Some((first, last)) = text.split_once("..=") {
        return Some((first.trim().parse().ok()?, last.trim().parse().ok()?));
    }
    let (first, last) = text.split_once("..")?;
    let last: u32 = last.trim().parse().ok()?;
    Some((first.trim().parse().ok()?, last.checked_sub(1)?))
}

/// What a task written `worker[i]` inside `repeat 1..=3 as i` stands for: `worker_1`,
/// `worker_2` and `worker_3`.
struct Pipes<'l>(&'l Language);

impl Resolver for Pipes<'_> {
    fn computed_names(
        &self,
        _: &SourceFile,
        node: &SyntaxNode,
    ) -> Option<Vec<(String, TextRange)>> {
        let language = self.0;
        let name = language.field(node, "name").into_iter().next()?;
        let index = language
            .field(node, "index")
            .into_iter()
            .next()?
            .to_string();
        let repeat = node.ancestors().find(|ancestor| {
            language.kind_name(ancestor.kind()) == "RepeatDecl"
                && language
                    .field(ancestor, "var")
                    .into_iter()
                    .next()
                    .is_some_and(|var| var.to_string() == index.trim())
        })?;
        let range = language.field(&repeat, "range").into_iter().next()?;
        let (first, last) = bounds(&range.to_string())?;
        let last = last.min(first.saturating_add(MOST - 1));
        let base = name.to_string();
        Some(
            (first..=last)
                .map(|number| (format!("{}_{number}", base.trim()), name.text_range()))
                .collect(),
        )
    }
}

const DEPLOY: &str = "pipeline deploy {\n  task build\n  task test { run build }\n  \
    repeat 1..=3 as i {\n    task worker[i]\n    label: \"Worker {i}\"\n    run worker_2\n  }\n}\n";
const CANARY: &str = "stage canary for pipeline deploy {\n  run build\n  run missing\n}\n";
const OTHER: &str = "pipeline deploy {\n}\n";

fn project(files: &[(&str, &str)]) -> (Project<'static>, Bindings) {
    let memory: MemoryFiles = files
        .iter()
        .map(|&(path, text)| (path.to_owned(), text.to_owned()))
        .collect();
    let mut project = Project::new(loaded());
    for (path, _) in files {
        project.open(path, &memory).expect("there");
    }
    for file in project.files() {
        assert_eq!(file.parse().diagnostics, [], "{}", file.source().name());
    }
    let bindings = Pipes(&loaded().language).resolve(&project);
    (project, bindings)
}

fn id(project: &Project<'_>, path: &str) -> FileId {
    project.file_at(path).expect("open").source().id()
}

fn at(text: &str, word: &str) -> TextSize {
    let found = text.find(word).expect("written");
    TextSize::new(u32::try_from(found).expect("small"))
}

fn resolver() -> Pipes<'static> {
    Pipes(&loaded().language)
}

#[test]
fn a_global_pipeline_is_seen_from_another_file_without_an_import() {
    let (project, bindings) = project(&[("deploy.pipe", DEPLOY), ("canary.pipe", CANARY)]);
    let (deploy, canary) = (id(&project, "deploy.pipe"), id(&project, "canary.pipe"));
    let found = bindings
        .definition_at(canary, at(CANARY, "deploy"))
        .expect("defined");
    assert_eq!((found.file, found.name.as_str()), (deploy, "deploy"));
    let uses: Vec<_> = bindings.references_to(found).map(|r| r.file).collect();
    assert_eq!(uses, [canary]);
    let edits = project
        .rename_with(&resolver(), &bindings, canary, at(CANARY, "deploy"), "ship")
        .expect("safe");
    let files: Vec<_> = edits.iter().map(|edit| edit.file).collect();
    assert_eq!(files, [deploy, canary]);
}

#[test]
fn a_second_global_pipeline_of_a_name_is_one_problem_showing_both() {
    let (project, bindings) = project(&[("a.pipe", DEPLOY), ("b.pipe", OTHER)]);
    let [found] = bindings.diagnostics() else {
        panic!("{:?}", bindings.diagnostics());
    };
    assert_eq!(found.code, "LM-B002");
    assert_eq!(found.message, "`deploy` is defined twice");
    assert_eq!(found.file, id(&project, "b.pipe"));
    assert_eq!(found.labels.len(), 1);
    assert_eq!(found.labels[0].file, id(&project, "a.pipe"));
    assert_eq!(found.labels[0].message, "first defined here");
}

#[test]
fn a_stage_sees_the_tasks_of_the_pipeline_it_is_for() {
    let (project, bindings) = project(&[("deploy.pipe", DEPLOY), ("canary.pipe", CANARY)]);
    let (deploy, canary) = (id(&project, "deploy.pipe"), id(&project, "canary.pipe"));
    let found = bindings
        .definition_at(canary, at(CANARY, "build"))
        .expect("the task of the pipeline");
    assert_eq!((found.file, found.name.as_str()), (deploy, "build"));
    let [problem] = bindings.diagnostics() else {
        panic!("{:?}", bindings.diagnostics());
    };
    assert_eq!(problem.message, "`missing` is not defined");
    let uses: Vec<_> = bindings.references_to(found).map(|r| r.file).collect();
    assert_eq!(uses, [deploy, canary]);
    let edits = project
        .rename_with(
            &resolver(),
            &bindings,
            canary,
            at(CANARY, "build"),
            "compile",
        )
        .expect("safe");
    assert_eq!(edits.len(), 3);
}

#[test]
fn a_repetition_defines_the_names_it_stands_for() {
    let (project, bindings) = project(&[("deploy.pipe", DEPLOY)]);
    let deploy = id(&project, "deploy.pipe");
    let names: Vec<_> = bindings
        .definitions()
        .iter()
        .filter(|definition| definition.computed)
        .map(|definition| definition.name.as_str())
        .collect();
    assert_eq!(names, ["worker_1", "worker_2", "worker_3"]);
    assert_eq!(bindings.diagnostics(), []);
    let used = bindings
        .definition_at(deploy, at(DEPLOY, "worker_2"))
        .expect("defined");
    assert_eq!(used.range.start(), at(DEPLOY, "worker[i]"));
    assert_eq!(bindings.references_to(used).count(), 1);
    let refused = project
        .rename_with(
            &resolver(),
            &bindings,
            deploy,
            at(DEPLOY, "worker_2"),
            "other",
        )
        .expect_err("refused");
    assert_eq!(refused, RenameError::Computed);
    assert_eq!(
        refused.to_string(),
        "this name is computed, so it cannot be renamed"
    );
}

#[test]
fn a_name_in_the_hole_of_a_text_leads_to_its_definition() {
    let (project, bindings) = project(&[("deploy.pipe", DEPLOY)]);
    let deploy = id(&project, "deploy.pipe");
    let hole = at(DEPLOY, "{i}") + TextSize::new(1);
    let found = bindings.definition_at(deploy, hole).expect("defined");
    assert_eq!(found.name, "i");
    assert_eq!(found.range.start(), at(DEPLOY, "as i") + TextSize::new(3));
    let edits = project
        .rename_with(&resolver(), &bindings, deploy, hole, "n")
        .expect("safe");
    // The definition, the use in the index of the task, and the one in the text.
    assert_eq!(edits.len(), 3);
}

struct Hooks;

impl LanguageHooks for Hooks {
    fn resolver(&self) -> Option<&dyn Resolver> {
        static PIPES: OnceLock<Pipes<'static>> = OnceLock::new();
        Some(PIPES.get_or_init(resolver))
    }
}

/// A conversation with a server, the way an editor has it.
struct Editor {
    server: Server<'static, Hooks>,
    id: i64,
}

impl Editor {
    fn new() -> Self {
        let mut files = MemoryFiles::default();
        files.insert("/work/deploy.pipe", DEPLOY);
        let mut editor = Self {
            server: Server::new(loaded(), Hooks).files(files),
            id: 0,
        };
        editor.ask(
            "initialize",
            json!({ "capabilities": {}, "rootUri": "file:///work" }),
        );
        editor
    }

    fn send(&mut self, message: Value) -> Vec<Value> {
        let replies = self.server.handle_json(&message.to_string());
        replies
            .iter()
            .map(|text| serde_json::from_str(text).expect("JSON"))
            .collect()
    }

    fn reply(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        let message =
            json!({ "jsonrpc": "2.0", "id": self.id, "method": method, "params": params });
        self.send(message).remove(0)
    }

    fn ask(&mut self, method: &str, params: Value) -> Value {
        self.reply(method, params)["result"].clone()
    }

    fn open(&mut self, name: &str, text: &str) -> Vec<Value> {
        let uri = format!("file:///work/{name}");
        let document = json!({ "uri": uri, "languageId": "pipes", "version": 1, "text": text });
        self.send(json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen",
                          "params": { "textDocument": document } }))
    }

    /// A request about the first place `word` is written in `text`, a little inside it.
    fn about(&mut self, method: &str, name: &str, text: &str, word: &str, extra: Value) -> Value {
        self.reply(method, self.params(name, text, word, extra))["result"].clone()
    }

    fn params(&self, name: &str, text: &str, word: &str, mut params: Value) -> Value {
        let before = &text[..text.find(word).expect("written") + 1];
        let line = before.matches('\n').count();
        let column = before.rsplit('\n').next().expect("a line").len() - 1;
        params["textDocument"] = json!({ "uri": format!("file:///work/{name}") });
        params["position"] = json!({ "line": line, "character": column });
        params
    }
}

fn file_of(value: &Value) -> &str {
    value["uri"]
        .as_str()
        .and_then(|uri| uri.rsplit('/').next())
        .expect("a file")
}

#[test]
fn the_editor_goes_to_finds_and_renames_every_kind_of_name() {
    let mut editor = Editor::new();
    let opened = editor.open("canary.pipe", CANARY);
    let found = &opened[0]["params"]["diagnostics"];
    assert_eq!(found.as_array().map(Vec::len), Some(1), "{found}");
    assert_eq!(
        found[0]["message"],
        "`missing` is not defined\nTo fix this, define `missing` before it is used, check the \
         spelling, or import the file that defines it."
    );

    // A global name and a name the stage sees are in the other file.
    for word in ["deploy", "build"] {
        let found = editor.about(
            "textDocument/definition",
            "canary.pipe",
            CANARY,
            word,
            json!({}),
        );
        assert_eq!(file_of(&found), "deploy.pipe", "{word}");
    }
    editor.open("deploy.pipe", DEPLOY);
    // A computed name leads to the one place it is written, and a name in a text to its
    // definition.
    let found = editor.about(
        "textDocument/definition",
        "deploy.pipe",
        DEPLOY,
        "worker_2",
        json!({}),
    );
    assert_eq!(found["range"]["start"]["line"], 4);
    let found = editor.about(
        "textDocument/definition",
        "deploy.pipe",
        DEPLOY,
        "i}\"",
        json!({}),
    );
    assert_eq!(found["range"]["start"]["line"], 3);

    let uses = editor.about(
        "textDocument/references",
        "canary.pipe",
        CANARY,
        "build",
        json!({ "context": { "includeDeclaration": true } }),
    );
    assert_eq!(uses.as_array().map(Vec::len), Some(3), "{uses}");
    let edit = editor.about(
        "textDocument/rename",
        "canary.pipe",
        CANARY,
        "build",
        json!({ "newName": "compile" }),
    );
    assert_eq!(
        edit["changes"].as_object().map(|files| files.len()),
        Some(2)
    );

    // The computed name is refused with a reason, and nothing is offered to rename.
    let params = editor.params(
        "deploy.pipe",
        DEPLOY,
        "worker_2",
        json!({ "newName": "other" }),
    );
    let refused = editor.reply("textDocument/rename", params);
    assert_eq!(
        refused["error"]["message"],
        "this name is computed, so it cannot be renamed"
    );
    let ready = editor.about(
        "textDocument/prepareRename",
        "deploy.pipe",
        DEPLOY,
        "worker_2",
        json!({}),
    );
    assert!(ready.is_null());
}
