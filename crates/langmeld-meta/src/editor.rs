//! What editors learn about grammar files themselves.

use std::sync::OnceLock;

use langmeld_syntax::{
    Diagnostic, FileId, Language, Parse, SyntaxElement, SyntaxNode, SyntaxToken, TextRange,
    TextSize,
};

use crate::declared::{Editor, EditorRule, Place};
use crate::meta;
use crate::problem::problem;

/// The language of `.langmeld` files, so that editors can serve grammar files like any
/// language. Its outline and folds are in [`grammar_editor`].
pub fn grammar_language() -> &'static Language {
    meta::language()
}

/// What the language of `.langmeld` files tells editors: an outline of the
/// declarations, and what folds.
pub fn grammar_editor() -> &'static Editor {
    static EDITOR: OnceLock<Editor> = OnceLock::new();
    EDITOR.get_or_init(|| {
        let place = Place::default();
        let symbol = |node: &str, kind: &str| EditorRule::Symbol {
            node: node.to_owned(),
            kind: kind.to_owned(),
            name: "name".to_owned(),
            place,
        };
        let folds = [
            "LanguageDecl",
            "TokensDecl",
            "ExprDecl",
            "BindDecl",
            "EditorDecl",
            "CommentsDecl",
            "TerminatorsDecl",
        ];
        Editor {
            rules: vec![
                symbol("LanguageDecl", "module"),
                symbol("SkipToken", "constant"),
                symbol("TokenDef", "constant"),
                symbol("RuleDecl", "class"),
                symbol("ExprDecl", "enum"),
                symbol("ExternToken", "constant"),
                symbol("ExternRule", "function"),
                EditorRule::Fold {
                    nodes: folds.map(str::to_owned).to_vec(),
                    place,
                },
            ],
        }
    })
}

fn range(place: Place) -> (FileId, TextRange) {
    (
        FileId::new(place.file),
        TextRange::new(TextSize::from(place.start), TextSize::from(place.end)),
    )
}

/// The problems of an `editor` block: lines that name nodes or labels the language does
/// not have, which would otherwise silently do nothing.
pub(crate) fn check(language: &Language, editor: &Editor) -> Vec<Diagnostic> {
    let schema = language.schema();
    let is_node = |name: &str| schema.node(name).is_some() || schema.union(name).is_some();
    let mut found = Vec::new();
    let unknown = |word: &str, node: &str, place: Place| {
        let (file, at) = range(place);
        problem(
            "LM-F016",
            file,
            at,
            format!("`{word}` names `{node}`, which is not a rule or expression of the language"),
            "write the name of a rule, or of an expression, as declared in the file",
        )
    };
    for rule in &editor.rules {
        match rule {
            EditorRule::Symbol {
                node, name, place, ..
            } => {
                if !is_node(node) {
                    found.push(unknown("symbol", node, *place));
                } else if let Some(schema) = schema.node(node)
                    && schema.field(name).is_none()
                {
                    let (file, at) = range(*place);
                    let labels: Vec<_> = schema
                        .fields
                        .iter()
                        .map(|f| format!("`{}`", f.name))
                        .collect();
                    let known = if labels.is_empty() {
                        "it has no labelled parts".to_owned()
                    } else {
                        format!("its labelled parts are {}", labels.join(", "))
                    };
                    found.push(problem(
                        "LM-F017",
                        file,
                        at,
                        format!(
                            "`{node}` has no part labelled `{name}`, so it has no name to list"
                        ),
                        format!(
                            "label the part that holds the name in the rule, as in `{name}:ident`, \
                             or name another part; {known}"
                        ),
                    ));
                }
            }
            EditorRule::Fold { nodes, place } => {
                for node in nodes.iter().filter(|node| !is_node(node)) {
                    found.push(unknown("fold", node, *place));
                }
            }
            EditorRule::Highlight { node, place, .. } => {
                if !is_node(node) {
                    found.push(unknown("highlight", node, *place));
                }
            }
        }
    }
    found
}

/// The names declared in a grammar file: the token of each.
fn declared(root: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    let language = meta::language();
    const DECLARING: [&str; 6] = [
        "RuleDecl",
        "ExprDecl",
        "ExternRule",
        "ExternToken",
        "TokenDef",
        "SkipToken",
    ];
    root.descendants()
        .filter(|node| DECLARING.contains(&language.kind_name(node.kind())))
        .filter_map(|node| {
            language
                .field(&node, "name")
                .into_iter()
                .find_map(SyntaxElement::into_token)
        })
}

/// Where the rule, expression or token named at `offset` is declared, in the same
/// file. `parse` comes from [`grammar_language`].
pub fn declaration(parse: &Parse, offset: TextSize) -> Option<TextRange> {
    let language = meta::language();
    let root = parse.syntax();
    let name = root
        .token_at_offset(offset)
        .into_iter()
        .find(|token| language.kind_name(token.kind()) == "ident")?;
    declared(&root)
        .find(|declared| declared.text() == name.text())
        .map(|declared| declared.text_range())
}
