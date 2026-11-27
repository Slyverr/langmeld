//! What editors ask of a language.

use langmeld_syntax::pattern::{field, lit, many, opt, r, sep};
use langmeld_syntax::{
    Assoc, ExprSpec, FileId, IdentStyle, IntPreset, Language, Preset, SourceFile, StringPreset,
    Token, TokenClass, parse, seq,
};

fn language() -> Language {
    let mut string = StringPreset::default();
    string.double = true;
    Language::builder("Editing")
        .token(Token::skip("space", r"[ \n]+"))
        .token(Token::preset("name", Preset::Ident(IdentStyle::Xid)))
        .token(Token::preset("int", Preset::Int(IntPreset::default())))
        .token(Token::preset("text", Preset::String(string)))
        .line_comment("//")
        .doc_comment("///")
        .contextual(["from"])
        .rule("File", many(r("Stmt")))
        .rule(
            "Stmt",
            seq![
                lit("let"),
                field("name", r("Name")),
                opt(seq![lit("from"), field("source", r("Name"))]),
                lit("="),
                field("values", sep(r("Expr"), ",")),
                lit(";")
            ],
        )
        .rule("Name", r("name"))
        .expr(
            ExprSpec::new("Expr")
                .atom("Literal", r("int"))
                .atom("Text", r("text"))
                .atom("Truth", lit("true"))
                .infix("And", ["and"], 1, Assoc::Left)
                .infix("Add", ["+"], 2, Assoc::Left),
        )
        .build()
        .expect("valid")
}

fn expected(language: &Language, text: &str) -> Vec<String> {
    let mut names: Vec<_> = language
        .expected(text)
        .into_iter()
        .map(|kind| language.kind_name(kind).to_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn tokens_have_classes() {
    let language = language();
    let source = "/// d\n// c\nlet x from y = 1, \"a\", true and 2 + 3;";
    let file = SourceFile::new(FileId::new(0), "x", source).expect("small");
    let parsed = parse(&language, &file);
    assert_eq!(parsed.diagnostics, []);
    let classes: Vec<_> = parsed
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter_map(|token| {
            let class = language.token_class(token.kind())?;
            (class != TokenClass::Whitespace).then(|| format!("{} {class:?}", token.text()))
        })
        .collect();
    assert_eq!(
        classes,
        [
            "/// d DocComment",
            "// c Comment",
            "let Keyword",
            "x Identifier",
            "from ContextualKeyword",
            "y Identifier",
            "= Punctuation",
            "1 Number",
            ", Punctuation",
            "\"a\" String",
            ", Punctuation",
            "true Keyword",
            "and Keyword",
            "2 Number",
            "+ Punctuation",
            "3 Number",
            "; Punctuation",
        ]
    );
}

#[test]
fn the_words_that_may_come_next_are_known() {
    let language = language();
    assert_eq!(expected(&language, ""), ["let"]);
    assert_eq!(expected(&language, "let x "), ["=", "from"]);
    let values = expected(&language, "let x = ");
    assert!(values.contains(&"true".to_owned()), "{values:?}");
    assert!(values.contains(&"int".to_owned()), "{values:?}");
    let after = expected(&language, "let x = 1 ");
    for wanted in [";", ",", "and", "+"] {
        assert!(after.contains(&wanted.to_owned()), "{wanted}: {after:?}");
    }
    assert!(!after.contains(&"let".to_owned()), "{after:?}");
    assert_eq!(expected(&language, "let x = 1; "), ["let"]);
    // After a mistake before the end, what comes at the end is still known.
    assert_eq!(expected(&language, "let = 1; let y = 2; "), ["let"]);
}

#[test]
fn labelled_parts_are_found_by_name() {
    let language = language();
    let file = SourceFile::new(FileId::new(0), "x", "let x from y = 1, 2;").expect("small");
    let root = parse(&language, &file).syntax();
    let statement = root.first_child().expect("a statement");
    let texts = |label| -> Vec<String> {
        language
            .field(&statement, label)
            .iter()
            .map(|element| element.to_string())
            .collect()
    };
    assert_eq!(texts("name"), ["x"]);
    assert_eq!(texts("source"), ["y"]);
    assert_eq!(texts("values"), ["1", "2"]);
    assert_eq!(texts("nothing"), Vec::<String>::new());
}

#[test]
fn comments_words_and_quotes_are_listed() {
    let language = language();
    assert_eq!(language.line_comments().collect::<Vec<_>>(), ["//", "///"]);
    assert_eq!(language.block_comments().count(), 0);
    let mut words: Vec<_> = language.words().collect();
    words.sort_unstable();
    assert_eq!(words, ["and", "from", "let", "true"]);
    assert_eq!(language.quotes().collect::<Vec<_>>(), ['"']);
}
