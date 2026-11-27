//! What editors learn about grammar files themselves.

use std::sync::OnceLock;

use langmeld_syntax::{
    Language, Parse, SyntaxElement, SyntaxNode, SyntaxToken, TextRange, TextSize,
};

use crate::declared::{Editor, EditorRule, Place};
use crate::meta;

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
