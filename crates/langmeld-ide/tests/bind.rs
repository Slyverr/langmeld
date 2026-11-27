//! Names: where they are introduced, what uses them, and renaming them.

use std::collections::BTreeMap;

use langmeld_ide::bind::{Bindings, Member, MemberAccess, Project, RenameError, Resolver};
use langmeld_ide::project::MemoryFiles;
use langmeld_meta::{Loaded, Source, load_language};
use langmeld_syntax::{Severity, SourceFile, TextRange, TextSize};

const GRAMMAR: &str = r#"langmeld 1
language Names { extensions "names" }
tokens {
    skip ws = /[ \n]+/
    name    = preset ident(ascii)
    text    = preset string(double)
    int     = preset int(dec)
}
rule File   = Item*
rule Item   = Use | Let | Fn | Block | Group | Extend | Show
rule Use    = "use" path:text ";"
rule Let    = "let" name:Name "=" value:Value ";"
rule Fn     = "fn" name:Name "(" param:Param? ")" body:Block
rule Param  = name:Name
rule Block  = "{" Item* "}"
rule Group  = "group" name:Name body:Members
rule Members = "{" Item* "}"
rule Extend = "extend" target:GroupRef body:Members
rule Show   = "show" value:Value ";"
rule Value  = NameRef | int
rule Name   = name
rule NameRef = name
rule GroupRef = name

bind {
    scope File, Block, Fn, Members
    Let.name   defines value after
    Fn.name    defines value hoisted in parent
    Param.name defines value
    Group.name defines group unique global
    NameRef    refers value
    GroupRef   refers group
    Use        imports value from Use.path
    Extend.body includes members of target
}
"#;

fn language() -> Loaded {
    load_language(Source::text("names.langmeld", GRAMMAR)).unwrap_or_else(|error| panic!("{error}"))
}

/// The files of a project, bound.
fn bound<'l>(language: &'l Loaded, files: &[(&str, &str)]) -> (Project<'l>, Bindings) {
    let memory: MemoryFiles = files
        .iter()
        .map(|&(path, text)| (path.to_owned(), text.to_owned()))
        .collect();
    let mut project = Project::new(language);
    for (path, _) in files {
        project.open(path, &memory).expect("read");
    }
    let bindings = project.bind();
    (project, bindings)
}

fn file<'p>(project: &'p Project<'_>, path: &str) -> &'p SourceFile {
    project.file_at(path).expect("open").source()
}

/// The range of the `nth` place `word` is written in a file.
fn written(project: &Project<'_>, path: &str, word: &str, nth: usize) -> (SourceFile, TextRange) {
    let source = file(project, path);
    let at = source
        .text()
        .match_indices(word)
        .nth(nth)
        .unwrap_or_else(|| panic!("{word} {nth} in {path}"))
        .0;
    let start = TextSize::new(at as u32);
    (source.clone(), TextRange::at(start, TextSize::of(word)))
}

/// Where the name at the `nth` `word` leads, as `path:line`.
fn goes(project: &Project<'_>, bindings: &Bindings, path: &str, word: &str, nth: usize) -> String {
    let (source, range) = written(project, path, word, nth);
    match bindings.definition_at(source.id(), range.start()) {
        Some(found) => {
            let target = project.file(found.file).expect("known").source();
            let line = target
                .line_index()
                .line_col(found.range.start())
                .expect("there")
                .line;
            format!("{}:{line}", target.name())
        }
        None => "nothing".to_owned(),
    }
}

fn problems(project: &Project<'_>, bindings: &Bindings) -> Vec<String> {
    bindings
        .diagnostics()
        .iter()
        .map(|found| {
            let source = project.file(found.file).expect("known").source();
            let line = source
                .line_index()
                .line_col(found.range.start())
                .expect("there")
                .line;
            format!("{} {}:{line} {}", found.code, source.name(), found.message)
        })
        .collect()
}

#[test]
fn the_innermost_name_visible_wins() {
    let language = language();
    let text = "let x = 1;\n{\n  let x = 2;\n  show x;\n}\nshow x;\nlet x = x;\nshow x;\n";
    let (project, bindings) = bound(&language, &[("a", text)]);
    assert_eq!(problems(&project, &bindings), Vec::<String>::new());
    assert_eq!(goes(&project, &bindings, "a", "show x", 0), "nothing");
    assert_eq!(goes(&project, &bindings, "a", "x;", 0), "a:2");
    assert_eq!(goes(&project, &bindings, "a", "x;", 1), "a:0");
    // A name defined `after` its statement is not visible inside it.
    assert_eq!(goes(&project, &bindings, "a", "x;", 2), "a:0");
    assert_eq!(goes(&project, &bindings, "a", "x;", 3), "a:6");
    // A definition leads to itself.
    assert_eq!(goes(&project, &bindings, "a", "x =", 1), "a:2");
}

#[test]
fn hoisted_names_are_visible_everywhere_and_parameters_inside() {
    let language = language();
    let text = "show f;\nfn f(p) {\n  show p;\n  show f;\n}\nshow p;\n";
    let (project, bindings) = bound(&language, &[("a", text)]);
    assert_eq!(goes(&project, &bindings, "a", "f;", 0), "a:1");
    assert_eq!(goes(&project, &bindings, "a", "p;", 0), "a:1");
    assert_eq!(goes(&project, &bindings, "a", "f;", 1), "a:1");
    assert_eq!(
        problems(&project, &bindings),
        ["LM-B001 a:5 `p` is not defined"]
    );
}

#[test]
fn unknown_names_suggest_the_closest() {
    let language = language();
    let (project, bindings) = bound(&language, &[("a", "let width = 1;\nshow widht;\n")]);
    assert_eq!(
        problems(&project, &bindings),
        ["LM-B001 a:1 `widht` is not defined"]
    );
    let found = &bindings.diagnostics()[0];
    assert_eq!(found.notes, ["did you mean `width`?"]);
    assert_eq!(found.fixes[0].edits[0].replacement, "width");
}

#[test]
fn global_names_cross_files_and_unique_ones_cannot_repeat() {
    let language = language();
    let files = [
        ("a", "group shapes {\n  let side = 1;\n}\n"),
        (
            "b",
            "extend shapes {\n  show side;\n}\nshow side;\ngroup shapes {}\n",
        ),
    ];
    let (project, bindings) = bound(&language, &files);
    assert_eq!(goes(&project, &bindings, "b", "shapes", 0), "a:0");
    // Inside `extend`, the members of the group are visible.
    assert_eq!(goes(&project, &bindings, "b", "side;", 0), "a:1");
    assert_eq!(
        problems(&project, &bindings),
        [
            "LM-B001 b:3 `side` is not defined",
            "LM-B002 b:4 `shapes` is defined twice",
        ]
    );
    let twice = &bindings.diagnostics()[1];
    assert_eq!(twice.labels.len(), 1);
    assert_ne!(twice.labels[0].file, twice.file);
}

#[test]
fn imports_bring_in_the_names_of_another_file() {
    let language = language();
    let files = [
        ("lib/a", "let shared = 1;\n{ let hidden = 2; }\n"),
        (
            "main",
            "use \"lib/a\";\nshow shared;\nshow hidden;\nuse \"gone\";\n",
        ),
    ];
    let (project, bindings) = bound(&language, &files);
    assert_eq!(goes(&project, &bindings, "main", "shared;", 0), "lib/a:0");
    assert_eq!(
        problems(&project, &bindings),
        [
            "LM-B001 main:2 `hidden` is not defined",
            "LM-B003 main:3 the file `gone` cannot be read",
        ]
    );
}

#[test]
fn references_and_completion() {
    let language = language();
    let text = "let a = 1;\nfn f(b) {\n  let c = a;\n  show c;\n}\nshow a;\n";
    let (project, bindings) = bound(&language, &[("m", text)]);
    let (source, range) = written(&project, "m", "a =", 0);
    let definition = bindings
        .definition_at(source.id(), range.start())
        .expect("defined");
    let lines: Vec<_> = bindings
        .references_to(definition)
        .map(|reference| u32::from(reference.range.start()))
        .collect();
    assert_eq!(lines.len(), 2);
    let (source, inside) = written(&project, "m", "show c", 0);
    let mut visible: Vec<_> = bindings
        .visible(source.id(), inside.start(), "value")
        .map(|d| d.name.as_str())
        .collect();
    visible.sort_unstable();
    assert_eq!(visible, ["a", "b", "c", "f"]);
}

/// The text of every file after renaming the name at the `nth` `word`.
fn renamed(
    language: &Loaded,
    files: &[(&str, &str)],
    (path, word, nth): (&str, &str, usize),
    new: &str,
) -> Result<BTreeMap<String, String>, RenameError> {
    let (project, bindings) = bound(language, files);
    let (source, range) = written(&project, path, word, nth);
    let edits = project.rename(&bindings, source.id(), range.start(), new)?;
    let mut texts: BTreeMap<String, String> = files
        .iter()
        .map(|&(path, text)| (path.to_owned(), text.to_owned()))
        .collect();
    let mut edits = edits;
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.range.start()));
    for edit in edits {
        let name = project.file(edit.file).expect("known").source().name();
        texts.get_mut(name).expect("known").replace_range(
            usize::from(edit.range.start())..usize::from(edit.range.end()),
            &edit.replacement,
        );
    }
    Ok(texts)
}

#[test]
fn renaming_changes_every_use_across_files() {
    let language = language();
    let files = [
        ("lib", "let size = 1;\n"),
        ("main", "use \"lib\";\nshow size;\n{ show size; }\n"),
    ];
    let texts = renamed(&language, &files, ("main", "size", 0), "width").expect("safe");
    assert_eq!(texts["lib"], "let width = 1;\n");
    assert_eq!(
        texts["main"],
        "use \"lib\";\nshow width;\n{ show width; }\n"
    );
}

#[test]
fn renaming_refuses_what_would_change_meaning() {
    let language = language();
    let files = [("m", "let a = 1;\n{\n  let b = 2;\n  show a;\n}\nshow a;\n")];
    // `show a` inside the block would then mean the inner `b`.
    assert_eq!(
        renamed(&language, &files, ("m", "a =", 0), "b"),
        Err(RenameError::ChangesMeaning)
    );
    for bad in ["1x", "let", "a b", "", "\"a\""] {
        assert_eq!(
            renamed(&language, &files, ("m", "a =", 0), bad),
            Err(RenameError::NotAName),
            "{bad}"
        );
    }
    assert_eq!(
        renamed(&language, &files, ("m", "show", 0), "c"),
        Err(RenameError::NothingThere)
    );
    assert!(renamed(&language, &files, ("m", "b =", 0), "c").is_ok());
}

#[test]
fn broken_files_bind_what_they_can() {
    let language = language();
    for text in [
        "let = ;",
        "fn (",
        "show x",
        "{{{",
        "use ;",
        "extend { }",
        "",
    ] {
        let (_, bindings) = bound(&language, &[("a", text), ("b", "use \"a\";")]);
        let _ = bindings.diagnostics();
    }
    // Files that import each other.
    let files = [
        ("a", "use \"b\";\nlet x = y;"),
        ("b", "use \"a\";\nlet y = x;"),
    ];
    let (project, bindings) = bound(&language, &files);
    assert_eq!(problems(&project, &bindings), Vec::<String>::new());
}

const MEMBERS: &str = r#"langmeld 1
language Records { extensions "rec" }
tokens {
    skip ws = /[ \n]+/
    name    = preset ident(ascii)
}
rule File   = Item*
rule Item   = Record | Get
rule Record = "record" name:Name "{" Field* "}"
rule Field  = name:Name ";"
rule Get    = "get" object:NameRef "." member:Member ";"
rule Name   = name
rule NameRef = name
rule Member = name

bind {
    scope File, Record
    Record.name defines value in parent
    Field.name  defines field
    NameRef     refers value
    Get.member  refers member of object
}
"#;

/// Looks a member up among the fields of the record named.
struct Fields;

impl Resolver for Fields {
    fn member(&self, access: &MemberAccess<'_>) -> Member {
        let bindings = access.bindings;
        let Some(owner) = bindings.definition_index(access.file, access.owner.text_range().start())
        else {
            return Member::Unknown;
        };
        bindings
            .members_of(owner)
            .find(|&member| bindings.definitions()[member].name == access.name)
            .map_or(Member::Missing, Member::Is)
    }
}

#[test]
fn members_are_resolved_by_the_language() {
    let language = load_language(Source::text("records.langmeld", MEMBERS))
        .unwrap_or_else(|error| panic!("{error}"));
    let text = "record point { x; y; }\nget point.y;\nget point.z;\n";
    let mut project = Project::new(&language);
    let file = project.set("main", text).expect("small");
    // Without a hook, members are left alone.
    assert!(project.bind().diagnostics().is_empty());

    let bindings = Fields.resolve(&project);
    let codes: Vec<_> = bindings.diagnostics().iter().map(|d| d.code).collect();
    assert_eq!(codes, ["LM-B005"]);
    assert_eq!(bindings.diagnostics()[0].message, "there is no member `z`");
    let use_of_y = TextSize::of(&text[..text.find(".y").expect("written") + 1]);
    let found = bindings.definition_at(file, use_of_y).expect("a field");
    assert_eq!(
        (found.name.as_str(), found.namespace.as_str()),
        ("y", "field")
    );
    let edits = project
        .rename_with(&Fields, &bindings, file, use_of_y, "height")
        .expect("safe");
    assert_eq!(edits.len(), 2);
    assert_eq!(
        project.rename_with(&Fields, &bindings, file, use_of_y, "z"),
        Err(RenameError::ChangesMeaning)
    );
}

#[test]
fn unused_definitions_can_be_warned_about() {
    let language = language();
    let (_, bindings) = bound(&language, &[("main", "let used = 1;\nlet idle = used;\n")]);
    // Nothing is reported unless asked for.
    assert!(bindings.diagnostics().is_empty());
    let unused = bindings.unused();
    assert_eq!(unused.len(), 1);
    assert_eq!(unused[0].code, "LM-B004");
    assert!(unused[0].message.contains("`idle`"));
    assert_eq!(unused[0].severity, Severity::Warning);
}

/// Every part of a problem that a person reads, in one text.
fn told(found: &langmeld_syntax::Diagnostic) -> String {
    let mut text = format!("{}: {}", found.code, found.message);
    for note in &found.notes {
        text.push_str(&format!("\n  note: {note}"));
    }
    for label in &found.labels {
        text.push_str(&format!("\n  label: {}", label.message));
    }
    text
}

#[test]
fn every_problem_says_what_is_wrong_and_how_to_solve_it() {
    let language = language();
    let files = [
        ("a", "group shapes {}\nlet used = 1;\nlet idle = 2;\n"),
        (
            "b",
            "use \"a\";\ngroup shapes {}\nshow nowhere;\nshow used;\nuse \"gone\";\n",
        ),
    ];
    let (_, bindings) = bound(&language, &files);
    let found: Vec<_> = bindings.diagnostics().iter().map(told).collect();
    assert_eq!(
        found,
        [
            "LM-B002: `shapes` is defined twice\n  note: To fix this, rename one of the two, or \
             remove one.\n  label: first defined here",
            "LM-B001: `nowhere` is not defined\n  note: To fix this, define `nowhere` before it \
             is used, check the spelling, or import the file that defines it.",
            "LM-B003: the file `gone` cannot be read\n  note: To fix this, check the path: it is \
             written relative to the folder of this file, with `/`, and the file must exist and \
             be written in this language.",
        ]
    );
    let unused: Vec<_> = (bindings.unused().iter())
        .filter(|found| found.message.contains("idle"))
        .map(told)
        .collect();
    assert_eq!(
        unused,
        [
            "LM-B004: `idle` is never used\n  note: To fix this, use the name, or remove its \
             definition."
        ]
    );
}

#[test]
fn a_full_project_says_so_instead_of_naming_a_missing_file() {
    let language = language();
    let mut files = MemoryFiles::default();
    files.insert("main", "use \"one\";\nuse \"two\";\n");
    files.insert("one", "let a = 1;\n");
    files.insert("two", "let b = 1;\n");
    let mut project = Project::new(&language).most_files(2);
    project.open("main", &files).expect("read");
    let found: Vec<_> = project.bind().diagnostics().iter().map(told).collect();
    assert_eq!(
        found,
        [
            "LM-B003: `two` was not read: the project already has all the files it holds\n  \
             note: To fix this, import fewer files, or split the project in two."
        ]
    );
}

#[test]
fn a_file_over_the_limit_is_not_added() {
    let language = language();
    let mut project = Project::new(&language).largest_file(8);
    assert!(project.set("big", "let aaaaaaaaaa = 1;").is_none());
    assert!(project.set("small", "let a;").is_some());
    assert_eq!(project.files().count(), 1);
}

#[test]
fn a_removed_file_gives_its_number_to_the_next() {
    let language = language();
    let mut project = Project::new(&language);
    let first = project.set("a", "let a = 1;").expect("small");
    project.set("b", "let b = 1;").expect("small");
    project.remove("a");
    assert_eq!(project.set("c", "let c = 1;"), Some(first));
    assert_eq!(project.set("a", "let a = 1;").map(|id| id.index()), Some(2));
    assert_eq!(project.files().count(), 3);
}

#[test]
fn the_same_project_binds_the_same_way_every_time() {
    let language = language();
    let files = [
        ("a", "group g { let x = 1; }\nlet y = x;\nshow z;\n"),
        ("b", "extend g { show x; }\nuse \"a\";\nshow y;\n"),
    ];
    let (project, first) = bound(&language, &files);
    let again = project.bind();
    assert_eq!(first.diagnostics(), again.diagnostics());
    assert_eq!(first.definitions(), again.definitions());
    assert_eq!(first.references(), again.references());
}

const JOBS: &str = r#"langmeld 1
language Jobs { extensions "jobs" }
tokens {
    skip ws = /[ \n]+/
    name    = preset ident(ascii)
    int     = preset int(dec)
}
rule File    = Item*
rule Item    = Repeat | Task | Run
rule Repeat  = "repeat" count:int "as" var:Name "{" Item* "}"
rule Task    = "task" name:Name
rule Run     = "run" target:NameRef
rule Name    = name
rule NameRef = name

bind {
    scope File, Repeat
    Task.name  defines task computed
    NameRef    refers task
}
"#;

/// A task inside a repetition stands for as many names as the repetition counts.
struct Counted<'l>(&'l langmeld_syntax::Language);

impl Resolver for Counted<'_> {
    fn computed_names(
        &self,
        _: &SourceFile,
        node: &langmeld_syntax::SyntaxNode,
    ) -> Option<Vec<(String, TextRange)>> {
        let language = self.0;
        let name = language.field(node, "name").into_iter().next()?;
        let repeat = node
            .ancestors()
            .find(|ancestor| language.kind_name(ancestor.kind()) == "Repeat")?;
        let count = language.field(&repeat, "count").into_iter().next()?;
        let count: u32 = count.as_token()?.text().parse().ok()?;
        let base = match &name {
            langmeld_syntax::SyntaxElement::Token(token) => token.text().to_owned(),
            langmeld_syntax::SyntaxElement::Node(node) => node.first_token()?.text().to_owned(),
        };
        Some(
            (1..=count.min(1_000))
                .map(|number| (format!("{base}_{number}"), name.text_range()))
                .collect(),
        )
    }
}

#[test]
fn computed_names_come_from_the_language_and_cannot_be_renamed() {
    let language = load_language(Source::text("jobs.langmeld", JOBS))
        .unwrap_or_else(|error| panic!("{error}"));
    let mut project = Project::new(&language);
    let text = "repeat 3 as i { task worker run worker_2 run worker_9 }\ntask solo\nrun solo\n";
    let file = project.set("main", text).expect("small");
    let resolver = Counted(&language.language);
    let bindings = resolver.resolve(&project);
    let names: Vec<_> = bindings
        .definitions()
        .iter()
        .map(|d| (d.name.as_str(), d.computed))
        .collect();
    assert_eq!(
        names,
        [
            ("worker_1", true),
            ("worker_2", true),
            ("worker_3", true),
            ("solo", false)
        ]
    );
    let found: Vec<_> = bindings
        .diagnostics()
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(found, ["`worker_9` is not defined"]);
    let at = |word: &str| TextSize::of(&text[..text.find(word).expect("written")]);
    let target = bindings
        .definition_at(file, at("worker_2"))
        .expect("resolves");
    assert_eq!(target.name, "worker_2");
    // Going to the definition leads to the one written name, and so do the others.
    assert_eq!(
        target.range,
        TextRange::at(at("worker run"), TextSize::of("worker"))
    );
    assert_eq!(bindings.references_to(target).count(), 1);
    assert_eq!(
        project.rename_with(&resolver, &bindings, file, at("worker_2"), "other"),
        Err(RenameError::Computed)
    );
    // A task not in a repetition keeps the name as written.
    assert_eq!(
        bindings
            .definition_at(file, at("solo\nrun"))
            .map(|d| d.name.as_str()),
        Some("solo")
    );
}

#[test]
fn a_resolver_names_what_is_there_without_being_written() {
    struct Builtins;
    impl Resolver for Builtins {
        fn predefined(&self) -> &[(&str, &str)] {
            &[("value", "print")]
        }
    }
    let language = language();
    let mut project = Project::new(&language);
    project.set("a", "show print;\nshow prnt;\n");
    let bindings = Builtins.resolve(&project);
    let found: Vec<_> = bindings.diagnostics().iter().map(told).collect();
    assert_eq!(
        found,
        ["LM-B001: `prnt` is not defined\n  note: did you mean `print`?"]
    );
}

#[test]
fn a_duplicate_of_a_unique_name_in_one_scope_is_one_problem() {
    let grammar = GRAMMAR.replace(
        "Let.name   defines value after",
        "Let.name   defines value unique",
    );
    let language = load_language(Source::text("names.langmeld", grammar))
        .unwrap_or_else(|error| panic!("{error}"));
    let (project, bindings) = bound(
        &language,
        &[("a", "let x = 1;\nlet x = 2;\n{ let x = 3; }\n")],
    );
    assert_eq!(
        problems(&project, &bindings),
        ["LM-B002 a:1 `x` is defined twice"]
    );
}
