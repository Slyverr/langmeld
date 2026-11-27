//! The ground that typed views of syntax trees stand on.

use langmeld_syntax::ast::between;
use langmeld_syntax::pattern::{lit, many, r};
use langmeld_syntax::{
    FileId, IdentStyle, Language, LexCursor, Preset, SourceFile, StringPreset, SyntaxElement,
    Token, parse, seq,
};

fn words(elements: impl Iterator<Item = SyntaxElement>) -> Vec<String> {
    elements
        .filter_map(SyntaxElement::into_token)
        .map(|token| token.text().to_owned())
        .filter(|text| !text.trim().is_empty())
        .collect()
}

fn language() -> Language {
    let mut string = StringPreset::default();
    string.double = true;
    Language::builder("Parts")
        .token(Token::skip("space", r" +"))
        .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
        .token(Token::interpolated("text", string, "{", "}", "File"))
        .token(Token::external("shift", |cursor: &mut LexCursor<'_>| {
            cursor.rest().starts_with("<<").then_some(2)
        }))
        .rule(
            "File",
            seq![many(r("word")), lit(";"), many(r("word")), many(r("text"))],
        )
        .build()
        .expect("valid")
}

#[test]
fn children_between_two_tokens() {
    let language = language();
    let file = SourceFile::new(FileId::new(0), "x", "a b ; c d").expect("small");
    let root = parse(&language, &file).syntax();
    let semi = language.token_kind(";").expect("a literal");
    assert_eq!(words(between(&root, None, None)), ["a", "b", ";", "c", "d"]);
    assert_eq!(words(between(&root, Some(semi), None)), ["c", "d"]);
    assert_eq!(words(between(&root, None, Some(semi))), ["a", "b"]);
    assert_eq!(words(between(&root, Some(semi), Some(semi))), ["c", "d"]);
    // Nothing comes after a token that is not there.
    let missing = SourceFile::new(FileId::new(0), "x", "a b").expect("small");
    let root = parse(&language, &missing).syntax();
    assert_eq!(
        words(between(&root, Some(semi), None)),
        Vec::<String>::new()
    );
}

#[test]
fn a_language_names_its_tokens_and_custom_code() {
    let language = language();
    let declared: Vec<_> = language.declared_tokens().collect();
    // A string with holes is a rule, so it is a node and not a token.
    assert_eq!(declared, ["word", "shift"]);
    assert!(language.kind("text").is_some());
    assert_eq!(language.custom_tokens().collect::<Vec<_>>(), ["shift"]);
    assert_eq!(language.custom_rules().count(), 0);
    let word = language.token_kind("word").expect("a token");
    assert_eq!(language.kind_name(word), "word");
    assert_eq!(language.token_kind("File"), None);
}
