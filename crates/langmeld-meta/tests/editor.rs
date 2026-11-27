//! What editors learn about grammar files themselves.

use langmeld_meta::{EditorRule, declaration, grammar_editor, grammar_language};
use langmeld_syntax::{FileId, SourceFile, TextSize, parse};

const GRAMMAR: &str = "langmeld 1\nlanguage Sets { extensions \"s\" }\n\
    tokens { skip ws = / +/ name = preset ident(ascii) }\n\
    rule File = Set* Value\nrule Set = \"set\" name\n\
    expr Value { atom Word = name }\n\
    extern rule Odd -> Value first \"!\"\nrule Last = Odd\n";

#[test]
fn declarations_form_the_outline() {
    let language = grammar_language();
    assert_eq!(language.name(), "Langmeld");
    assert_eq!(language.extensions().collect::<Vec<_>>(), ["langmeld"]);
    let file = SourceFile::new(FileId::new(0), "a.langmeld", GRAMMAR).expect("small");
    let parsed = parse(language, &file);
    assert_eq!(parsed.diagnostics, []);
    let outline: Vec<&str> = grammar_editor()
        .rules
        .iter()
        .filter_map(|rule| match rule {
            EditorRule::Symbol { node, .. } => Some(node.as_str()),
            _ => None,
        })
        .collect();
    let mut found = Vec::new();
    for node in parsed.syntax().descendants() {
        let kind = language.kind_name(node.kind());
        if outline.contains(&kind) {
            let name = language.field(&node, "name");
            found.push(format!("{kind} {}", name[0]));
        }
    }
    assert_eq!(
        found,
        [
            "LanguageDecl Sets",
            "SkipToken ws",
            "TokenDef name",
            "RuleDecl File",
            "RuleDecl Set",
            "ExprDecl Value",
            "ExternRule Odd",
            "RuleDecl Last",
        ]
    );
}

#[test]
fn every_node_of_an_editor_rule_exists_in_the_language() {
    let language = grammar_language();
    for rule in &grammar_editor().rules {
        let nodes: Vec<&String> = match rule {
            EditorRule::Symbol { node, .. } => vec![node],
            EditorRule::Fold { nodes, .. } => nodes.iter().collect(),
            _ => continue,
        };
        for node in nodes {
            assert!(language.kind(node).is_some(), "no node `{node}`");
        }
    }
}

#[test]
fn a_name_leads_to_its_declaration() {
    let file = SourceFile::new(FileId::new(0), "a.langmeld", GRAMMAR).expect("small");
    let parsed = parse(grammar_language(), &file);
    let goes = |written: &str, nth: usize| {
        let at = GRAMMAR.match_indices(written).nth(nth).expect("written").0;
        let offset = TextSize::new(u32::try_from(at).expect("small") + 1);
        declaration(&parsed, offset).map(|range| {
            let start = usize::from(range.start());
            (start, &GRAMMAR[start..usize::from(range.end())])
        })
    };
    let declared = |text: &str| GRAMMAR.find(text).expect("there") + text.find(' ').expect("") + 1;
    assert_eq!(goes("Set*", 0), Some((declared("rule Set"), "Set")));
    assert_eq!(goes("Value\n", 0), Some((declared("expr Value"), "Value")));
    assert_eq!(goes("name\n", 0).map(|found| found.1), Some("name"));
    assert_eq!(goes("Odd\n", 0).map(|found| found.1), Some("Odd"));
    // A declaration leads to itself; words of the format lead nowhere.
    assert_eq!(goes("File", 0), Some((declared("rule File"), "File")));
    assert_eq!(goes("rule ", 0), None);
    assert_eq!(goes("first", 0), None);
}

const LISTED: &str = "langmeld 1\nlanguage T { extensions \"t\" }\n\
    tokens { skip ws = / +/ ident = preset ident(ascii) }\n\
    rule File = Item*\nrule Item = \"item\" title:ident\n";

fn refused(editor: &str) -> String {
    let text = format!("{LISTED}editor {{\n{editor}\n}}\n");
    match langmeld_meta::load_language(langmeld_meta::Source::text("t.langmeld", text)) {
        Ok(_) => panic!("accepted"),
        Err(error) => error.rendered(),
    }
}

#[test]
fn a_node_that_does_not_exist_is_refused_once() {
    assert_eq!(
        refused("  symbol Itme kind function name title"),
        "error[LM-F016]: `symbol` names `Itme`, which is not a rule or expression of the language\n \
         --> t.langmeld:7:3\n  |\n7 |   symbol Itme kind function name title\n  |   \
         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^\n  = note: To fix this, write the name of a rule, \
         or of an expression, as declared in the file.\n\n"
    );
}

#[test]
fn a_label_that_does_not_exist_is_refused_with_the_labels_there_are() {
    assert_eq!(
        refused("  symbol Item kind function name name"),
        "error[LM-F017]: `Item` has no part labelled `name`, so it has no name to list\n \
         --> t.langmeld:7:3\n  |\n7 |   symbol Item kind function name name\n  |   \
         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^\n  = note: To fix this, label the part that holds \
         the name in the rule, as in `name:ident`, or name another part; its labelled parts are \
         `title`.\n\n"
    );
}

#[test]
fn folds_and_highlights_of_missing_nodes_are_refused() {
    assert_eq!(
        refused("  fold File, Blok\n  highlight Nothing by_binding"),
        "error[LM-F016]: `fold` names `Blok`, which is not a rule or expression of the language\n \
         --> t.langmeld:7:3\n  |\n7 |   fold File, Blok\n  |   ^^^^^^^^^^^^^^^\n  = note: To \
         fix this, write the name of a rule, or of an expression, as declared in the file.\n\n\
         error[LM-F016]: `highlight` names `Nothing`, which is not a rule or expression of the \
         language\n --> t.langmeld:8:3\n  |\n8 |   highlight Nothing by_binding\n  |   \
         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^\n  = note: To fix this, write the name of a rule, or of an \
         expression, as declared in the file.\n\n"
    );
}

#[test]
fn right_lines_are_accepted() {
    let text =
        format!("{LISTED}editor {{\n  symbol Item kind function name title\n  fold File\n}}\n");
    let loaded = langmeld_meta::load_language(langmeld_meta::Source::text("t.langmeld", text))
        .expect("accepted");
    assert_eq!(loaded.editor.rules.len(), 2);
}
