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
