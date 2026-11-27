//! The grammar of `.langmeld` files, declared with the grammar builder that
//! `.langmeld` files are themselves a way to write.
//!
//! Read it top to bottom: the words and tokens first, then one rule for each thing a
//! file can hold, in the order a file holds them.

use std::sync::OnceLock;

use langmeld_syntax::pattern::{field, lit, many, many1, not, opt, r, sep_strict};
use langmeld_syntax::{
    Diagnostic, IdentStyle, IntPreset, Language, Preset, StringPreset, Token, choice, seq,
};

/// Words that only start a part of a file or fill one in. Each is still an ordinary
/// name anywhere else, so labels and tokens may be called `kind` or `name`.
const WORDS: [&str; 41] = [
    "after",
    "bind",
    "block",
    "by_binding",
    "comments",
    "computed",
    "contextual",
    "continue_inside",
    "defines",
    "doc",
    "editor",
    "expr",
    "extensions",
    "first",
    "fold",
    "from",
    "global",
    "highlight",
    "hoisted",
    "imports",
    "in",
    "includes",
    "interpolate",
    "keywords",
    "kind",
    "langmeld",
    "language",
    "left",
    "line",
    "members",
    "name",
    "nested",
    "newline",
    "none",
    "of",
    "parent",
    "preset",
    "refers",
    "right",
    "rule",
    "tokens",
];

/// More contextual words, which do not fit the line above.
const MORE_WORDS: [&str; 7] = [
    "shadowing",
    "skip",
    "symbol",
    "terminators",
    "token",
    "unique",
    "with",
];

/// Words that a rule cannot be written without, so they are never names. Between them,
/// they end every kind of declaration: a part of a rule is never one of these, so where
/// a rule stops is clear from the next word.
const RESERVED: [&str; 12] = [
    "atom",
    "extern",
    "import",
    "infix",
    "postfix",
    "prec",
    "prefix",
    "recover",
    "scope",
    "sep",
    "sep_strict",
    "try",
];

/// What follows the opening word of a block of declarations, as a rule.
fn block(word: &str, entry: &str) -> langmeld_syntax::Pattern {
    seq![lit(word), lit("{"), many(r(entry)), lit("}")]
}

fn build() -> Result<Language, Vec<Diagnostic>> {
    let mut quoted = StringPreset::default();
    quoted.double = true;
    Language::builder("Langmeld")
        // The tokens: names, numbers, quoted text and `/regular expressions/`.
        .token(Token::skip("space", r"[ \t\r\n]+"))
        .token(Token::preset("ident", Preset::Ident(IdentStyle::Ascii)))
        .token(Token::preset("number", Preset::Int(IntPreset::default())))
        .token(Token::preset("string", Preset::String(quoted)))
        .token(Token::regex("regex", r"/([^/\\\r\n]|\\.)+/"))
        .line_comment("//")
        .keywords(RESERVED)
        .contextual(WORDS.into_iter().chain(MORE_WORDS))
        // A file: the version line, then declarations in any order.
        .rule("File", seq![opt(r("Header")), many(r("Item"))])
        .rule(
            "Header",
            seq![lit("langmeld"), field("version", r("number"))],
        )
        .rule(
            "Item",
            choice![
                r("Import"),
                r("LanguageDecl"),
                r("TokensDecl"),
                r("RuleDecl"),
                r("ExprDecl"),
                r("ExternDecl"),
                r("BindDecl"),
                r("EditorDecl")
            ],
        )
        .rule("Import", seq![lit("import"), field("path", r("string"))])
        // `language Name { extensions "x"  comments { … }  terminators { … } }`
        .rule(
            "LanguageDecl",
            seq![
                lit("language"),
                field("name", r("ident")),
                lit("{"),
                many(r("LanguageSetting")),
                lit("}")
            ],
        )
        .rule(
            "LanguageSetting",
            choice![
                r("Extensions"),
                r("CommentsDecl"),
                r("TerminatorsDecl"),
                r("ContinueInside")
            ],
        )
        .rule("Extensions", seq![lit("extensions"), many1(r("string"))])
        .rule("CommentsDecl", block("comments", "CommentKind"))
        .rule(
            "CommentKind",
            choice![r("LineComment"), r("DocComment"), r("BlockComment")],
        )
        .rule("LineComment", seq![lit("line"), r("string")])
        .rule("DocComment", seq![lit("doc"), r("string")])
        .rule(
            "BlockComment",
            seq![lit("block"), r("string"), r("string"), opt(lit("nested"))],
        )
        .rule("TerminatorsDecl", block("terminators", "NewlineTerminator"))
        .rule("NewlineTerminator", seq![lit("newline"), many(r("string"))])
        .rule("ContinueInside", block("continue_inside", "BracketPair"))
        .rule("BracketPair", seq![r("string"), r("string")])
        // `tokens { skip ws = /…/  ident = preset ident(xid)  keywords { … } }`
        .rule("TokensDecl", block("tokens", "TokenItem"))
        .rule(
            "TokenItem",
            choice![
                r("SkipToken"),
                r("KeywordList"),
                r("ContextualList"),
                r("TokenDef")
            ],
        )
        .rule(
            "SkipToken",
            seq![
                lit("skip"),
                field("name", r("ident")),
                lit("="),
                r("TokenValue")
            ],
        )
        .rule(
            "TokenDef",
            seq![field("name", r("ident")), lit("="), r("TokenValue")],
        )
        .rule("KeywordList", block("keywords", "ident"))
        .rule("ContextualList", block("contextual", "ident"))
        .rule("TokenValue", choice![r("RegexValue"), r("PresetValue")])
        .rule("RegexValue", r("regex"))
        .rule(
            "PresetValue",
            seq![
                lit("preset"),
                field("kind", r("ident")),
                opt(seq![lit("("), sep_strict(r("PresetOption"), ","), lit(")")])
            ],
        )
        .rule("PresetOption", choice![r("Interpolate"), r("Flag")])
        .rule(
            "Interpolate",
            seq![
                lit("interpolate"),
                r("string"),
                r("string"),
                lit("with"),
                field("content", r("ident"))
            ],
        )
        .rule("Flag", r("ident"))
        // `rule Name = parts | other parts recover "}"`
        .rule(
            "RuleDecl",
            seq![
                lit("rule"),
                field("name", r("ident")),
                lit("="),
                field("body", r("Alternation")),
                opt(r("Recover"))
            ],
        )
        .rule("Recover", seq![lit("recover"), many1(r("string"))])
        .rule(
            "Alternation",
            seq![r("Sequence"), many(seq![lit("|"), r("Sequence")])],
        )
        // A part of a rule is never the start of the next declaration, so a rule with
        // nothing after its `=` ends there, and is reported as having no body.
        .rule("Sequence", many1(seq![not(r("Declaration")), r("Piece")]))
        .rule(
            "Declaration",
            choice![
                seq![lit("rule"), r("ident")],
                seq![lit("expr"), r("ident")],
                seq![lit("language"), r("ident")],
                seq![lit("tokens"), lit("{")],
                seq![lit("bind"), lit("{")],
                seq![lit("editor"), lit("{")]
            ],
        )
        .rule("Piece", choice![r("Look"), r("Part")])
        .rule("Look", seq![choice![lit("&"), lit("!")], r("Repeated")])
        .rule(
            "Part",
            seq![
                opt(seq![field("label", r("ident")), lit(":")]),
                r("Repeated")
            ],
        )
        .rule(
            "Repeated",
            seq![r("Atom"), opt(choice![lit("*"), lit("+"), lit("?")])],
        )
        .rule(
            "Atom",
            choice![
                r("Literal"),
                r("Group"),
                r("SepList"),
                r("Terminated"),
                r("Attempt"),
                r("Reference")
            ],
        )
        .rule("Literal", r("string"))
        .rule("Reference", r("ident"))
        .rule("Group", seq![lit("("), r("Alternation"), lit(")")])
        .rule(
            "SepList",
            seq![
                choice![lit("sep"), lit("sep_strict")],
                lit("("),
                r("Alternation"),
                lit(","),
                r("string"),
                lit(")")
            ],
        )
        .rule(
            "Terminated",
            seq![lit("terminated"), lit("("), r("Alternation"), lit(")")],
        )
        .rule(
            "Attempt",
            seq![lit("try"), lit("("), r("Alternation"), lit(")")],
        )
        // `expr Name { atom A = …  prefix B "-" prec 9  infix C "+" prec 5 left }`
        .rule(
            "ExprDecl",
            seq![
                lit("expr"),
                field("name", r("ident")),
                lit("{"),
                many(r("ExprEntry")),
                lit("}")
            ],
        )
        .rule(
            "ExprEntry",
            choice![
                r("AtomDecl"),
                r("PrefixDecl"),
                r("InfixDecl"),
                r("PostfixDecl")
            ],
        )
        .rule(
            "AtomDecl",
            seq![
                lit("atom"),
                field("name", r("ident")),
                lit("="),
                r("Alternation")
            ],
        )
        .rule(
            "PrefixDecl",
            seq![
                lit("prefix"),
                field("name", r("ident")),
                r("Operators"),
                lit("prec"),
                field("precedence", r("number"))
            ],
        )
        .rule(
            "InfixDecl",
            seq![
                lit("infix"),
                field("name", r("ident")),
                r("Operators"),
                lit("prec"),
                field("precedence", r("number")),
                r("Assoc")
            ],
        )
        .rule(
            "PostfixDecl",
            seq![
                lit("postfix"),
                field("name", r("ident")),
                r("Sequence"),
                lit("prec"),
                field("precedence", r("number"))
            ],
        )
        .rule(
            "Operators",
            seq![r("string"), many(seq![lit("|"), r("string")])],
        )
        .rule("Assoc", choice![lit("left"), lit("right"), lit("none")])
        // `extern token Name` and `extern rule Name -> Kind first "("`
        .rule(
            "ExternDecl",
            seq![lit("extern"), choice![r("ExternToken"), r("ExternRule")]],
        )
        .rule("ExternToken", seq![lit("token"), field("name", r("ident"))])
        .rule(
            "ExternRule",
            seq![
                lit("rule"),
                field("name", r("ident")),
                opt(seq![lit("->"), field("kind", r("ident"))]),
                opt(seq![lit("first"), many1(r("string"))])
            ],
        )
        // `bind { scope A, B  A.name defines value after  B refers value }`
        .rule("BindDecl", block("bind", "BindStatement"))
        .rule(
            "BindStatement",
            choice![r("ScopeStatement"), r("Statement")],
        )
        .rule(
            "ScopeStatement",
            seq![lit("scope"), r("ident"), many(seq![lit(","), r("ident")])],
        )
        .rule("Statement", seq![sep_strict(r("Path"), ","), r("Verb")])
        .rule("Path", seq![r("ident"), many(seq![lit("."), r("ident")])])
        .rule(
            "Verb",
            choice![r("Defines"), r("Refers"), r("Imports"), r("Includes")],
        )
        .rule(
            "Defines",
            seq![
                lit("defines"),
                field("namespace", r("ident")),
                many(r("Modifier"))
            ],
        )
        .rule(
            "Modifier",
            choice![
                lit("after"),
                lit("hoisted"),
                seq![lit("in"), lit("parent")],
                lit("shadowing"),
                lit("unique"),
                lit("global"),
                lit("computed")
            ],
        )
        .rule(
            "Refers",
            seq![lit("refers"), field("namespace", r("ident"))],
        )
        .rule(
            "Imports",
            seq![
                lit("imports"),
                field("namespace", r("ident")),
                lit("from"),
                r("Path")
            ],
        )
        .rule(
            "Includes",
            seq![lit("includes"), lit("members"), lit("of"), r("Path")],
        )
        // `editor { symbol A kind function name name  fold B  highlight C by_binding }`
        .rule("EditorDecl", block("editor", "EditorStatement"))
        .rule(
            "EditorStatement",
            choice![
                r("SymbolStatement"),
                r("FoldStatement"),
                r("HighlightStatement")
            ],
        )
        .rule(
            "SymbolStatement",
            seq![
                lit("symbol"),
                r("ident"),
                lit("kind"),
                r("ident"),
                lit("name"),
                r("ident")
            ],
        )
        .rule(
            "FoldStatement",
            seq![lit("fold"), sep_strict(r("ident"), ",")],
        )
        .rule(
            "HighlightStatement",
            seq![lit("highlight"), r("ident"), r("ident")],
        )
        // How a mistake names what it expected.
        .extensions(["langmeld"])
        .describe("Item", "a declaration")
        .describe("Piece", "a part of a rule")
        .describe("Atom", "a part of a rule")
        .describe("Sequence", "a part of a rule")
        .describe("ident", "a name")
        .describe("string", "quoted text")
        .describe("number", "a number")
        .describe("regex", "a `/pattern/`")
        .build()
}

/// The language of `.langmeld` files, built on first use.
pub(crate) fn language() -> &'static Language {
    static LANGUAGE: OnceLock<Language> = OnceLock::new();
    LANGUAGE.get_or_init(|| {
        build().expect("the grammar of `.langmeld` files is fixed, and the tests build it")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grammar_is_accepted_without_warnings() {
        let language = build().unwrap_or_else(|problems| panic!("{problems:#?}"));
        assert!(language.warnings().is_empty(), "{:?}", language.warnings());
        // Deciding where a declaration ends needs a few tokens, never more.
        assert!(language.max_lookahead() <= 3, "{:?}", language.lookahead());
    }
}
