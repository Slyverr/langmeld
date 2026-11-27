//! JSON declared as a grammar: the smallest useful example, and the measure of the
//! parser's speed.

use std::sync::OnceLock;

use langmeld_syntax::pattern::{choice, field, lit, r, sep_strict};
use langmeld_syntax::{Language, LanguageBuilder, Preset, StringPreset, Token, choice, seq};

/// The declarations of JSON, with `extra` further kinds of value, each starting with
/// a word of its own. The extra ones are not JSON: they show how speed depends on the
/// size of a grammar.
pub fn builder(extra: usize) -> LanguageBuilder {
    let mut string = StringPreset::default();
    string.double = true;
    let more = (0..extra).map(|n| format!("Extra{n}"));
    let values = ["Object", "Array", "Literal"]
        .map(str::to_owned)
        .into_iter()
        .chain(more.clone())
        .map(r);

    let json = Language::builder("Json")
        .token(Token::skip("space", r"[ \t\r\n]+"))
        .token(Token::preset("string", Preset::String(string)))
        .token(Token::regex(
            "number",
            r"-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?",
        ))
        // Needed to recognise `true`, `false` and `null` as words.
        .token(Token::regex("word", "[A-Za-z]+").identifier())
        .rule("File", field("value", r("Value")))
        .rule("Value", choice(values))
        .rule(
            "Object",
            seq![
                lit("{"),
                field("members", sep_strict(r("Member"), ",")),
                lit("}")
            ],
        )
        .rule(
            "Member",
            seq![
                field("key", r("string")),
                lit(":"),
                field("value", r("Value"))
            ],
        )
        .rule(
            "Array",
            seq![
                lit("["),
                field("items", sep_strict(r("Value"), ",")),
                lit("]")
            ],
        )
        .rule(
            "Literal",
            choice![
                r("string"),
                r("number"),
                lit("true"),
                lit("false"),
                lit("null")
            ],
        )
        .describe("Value", "a value")
        .describe("Member", "a member");
    more.enumerate().fold(json, |builder, (n, name)| {
        // A word of letters only, as JSON's words are.
        let word: String = [n / 676, n / 26, n]
            .map(|digit| char::from(b'a' + (digit % 26) as u8))
            .into_iter()
            .collect();
        builder.rule(name, seq![lit(format!("extra{word}")), r("Value")])
    })
}

fn build() -> Language {
    builder(0)
        .build()
        .expect("the JSON grammar is fixed, and the tests build it")
}

/// The JSON language, built on first use.
///
/// ```
/// use langmeld_syntax::{FileId, SourceFile};
///
/// let file = SourceFile::new(FileId::new(0), "a.json", r#"{"a": [1, true]}"#)?;
/// assert!(langmeld_syntax::parse(json::language(), &file).diagnostics.is_empty());
/// # Ok::<(), langmeld_syntax::TextTooLarge>(())
/// ```
pub fn language() -> &'static Language {
    static LANGUAGE: OnceLock<Language> = OnceLock::new();
    LANGUAGE.get_or_init(build)
}
