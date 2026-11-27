//! A language written as bytes and read back parses exactly like the original.

#![cfg(not(target_arch = "wasm32"))]

use langmeld_syntax::pattern::{field, lit, many, r, terminated};
use langmeld_syntax::{
    Assoc, EmbedError, ExprSpec, Externals, FileId, IdentStyle, IntPreset, Language, Parser,
    Preset, SourceFile, StringPreset, SyntaxKind, Token, parse, seq,
};

fn language() -> Language {
    let mut quotes = StringPreset::default();
    quotes.double = true;
    Language::builder("Embedded")
        .token(Token::skip("space", r"[ \t\r\n]+"))
        .token(Token::preset("name", Preset::Ident(IdentStyle::Xid)))
        .token(Token::preset("number", Preset::Int(IntPreset::default())))
        .token(Token::interpolated("text", quotes, "{", "}", "Expr"))
        .token(Token::external("raw", |cursor| {
            cursor.rest().strip_prefix('#').map(|_| 1)
        }))
        .line_comment("//")
        .keywords(["let"])
        .rule("File", many(terminated(r("Item"))))
        .rule(
            "Item",
            seq![lit("let"), field("name", r("name")), lit("="), r("Expr")],
        )
        .expr(
            ExprSpec::new("Expr")
                .atom("Number", r("number"))
                .atom("Text", r("text"))
                .atom("Raw", r("raw"))
                .atom("Twice", r("Pair"))
                .infix("Sum", ["+", "-"], 1, Assoc::Left)
                .infix("Product", ["*"], 2, Assoc::Left),
        )
        .extern_rule(
            langmeld_syntax::ExternRule::new("Pair", |p: &mut Parser<'_>| {
                let node = p.start();
                p.bump();
                node.complete(p, SyntaxKind::ERROR);
            })
            .first(["twice"]),
        )
        .build()
        .expect("the grammar is valid")
}

fn externals() -> Externals {
    Externals::new()
        .rule("Pair", |p| {
            let node = p.start();
            p.bump();
            node.complete(p, SyntaxKind::ERROR);
        })
        .token("raw", |cursor| cursor.rest().strip_prefix('#').map(|_| 1))
}

fn shown(language: &Language, text: &str) -> String {
    let file = SourceFile::new(FileId::new(0), "a", text).expect("small");
    let parsed = parse(language, &file);
    let mut out = format!("{:#?}\n", parsed.syntax());
    for d in &parsed.diagnostics {
        out.push_str(&format!("{} {:?} {}\n", d.code, d.range, d.message));
    }
    out
}

const TEXTS: [&str; 6] = [
    "let a = 1 + 2 * 3\nlet b = \"x {a + 1} y\"",
    "let a = \n let",
    "// note\nlet c = \"{",
    "let d = 1 +\n\n 2",
    "let e = ) 3 let",
    "",
];

#[test]
fn a_language_read_back_parses_the_same() {
    let original = language();
    let bytes = original.to_bytes().expect("written");
    let again = Language::from_bytes(&bytes, &externals()).expect("read back");
    for text in TEXTS {
        assert_eq!(shown(&original, text), shown(&again, text), "{text:?}");
    }
    assert_eq!(original.lookahead(), again.lookahead());
    assert_eq!(original.schema(), again.schema());
}

#[test]
fn the_same_language_gives_the_same_bytes() {
    let first = language().to_bytes().expect("written");
    let second = language().to_bytes().expect("written");
    assert_eq!(first, second);
}

#[test]
fn bytes_that_cannot_be_read_are_refused_by_name() {
    let bytes = language().to_bytes().expect("written");
    assert_eq!(
        Language::from_bytes(b"nothing", &externals()).expect_err("not a language"),
        EmbedError::NotALanguage
    );
    let mut damaged = bytes.clone();
    let last = damaged.len() - 1;
    damaged[last] ^= 0xff;
    assert_eq!(
        Language::from_bytes(&damaged, &externals()).expect_err("damaged"),
        EmbedError::Damaged
    );
    let mut other = bytes.clone();
    other[5] = b'9';
    assert!(matches!(
        Language::from_bytes(&other, &externals()).expect_err("other version"),
        EmbedError::OtherVersion(_)
    ));
    assert_eq!(
        Language::from_bytes(&bytes, &Externals::new()).expect_err("no code"),
        EmbedError::MissingToken("raw".to_owned())
    );
    let only_token = Externals::new().token("raw", |_| None);
    assert_eq!(
        Language::from_bytes(&bytes, &only_token).expect_err("no rule code"),
        EmbedError::MissingRule("Pair".to_owned())
    );
    assert_eq!(
        EmbedError::MissingRule("Pair".to_owned()).to_string(),
        "custom rule `Pair` has no code"
    );
}
