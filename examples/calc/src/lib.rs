//! Calc, a small language declared as a grammar. The same language written by hand is
//! the `handwritten-calc` example.

use std::sync::OnceLock;

use langmeld_syntax::pattern::{field, lit, look, many, r, sep};
use langmeld_syntax::{
    Assoc, ExprSpec, FloatPreset, IdentStyle, IntPreset, Language, Preset, StringPreset, Token,
    choice, seq,
};

fn expressions() -> ExprSpec {
    ExprSpec::new("Expr")
        .atom(
            "Literal",
            choice![r("int"), r("float"), r("string"), lit("true"), lit("false")],
        )
        .atom("Paren", seq![lit("("), field("inner", r("Expr")), lit(")")])
        .atom("Var", r("NameRef"))
        .atom(
            "If",
            seq![
                lit("if"),
                field("cond", r("Expr")),
                field("then", r("Block")),
                langmeld_syntax::pattern::opt(seq![lit("else"), field("else", r("Block"))])
            ],
        )
        .prefix("Neg", ["-"], 9)
        .infix("Mul", ["*", "/"], 6, Assoc::Left)
        .infix("Add", ["+", "-"], 5, Assoc::Left)
        .infix("Cmp", ["==", "<"], 4, Assoc::None)
        .postfix(
            "Call",
            seq![lit("("), field("args", sep(r("Expr"), ",")), lit(")")],
            10,
        )
}

fn build() -> Language {
    let mut int = IntPreset::default();
    int.hex = true;
    int.underscores = true;
    let mut float = FloatPreset::default();
    float.exponent = true;
    let mut string = StringPreset::default();
    string.double = true;

    Language::builder("Calc")
        .token(Token::skip("ws", r"[ \t\r\n]+"))
        .token(Token::preset("ident", Preset::Ident(IdentStyle::Xid)))
        .token(Token::preset("int", Preset::Int(int)))
        .token(Token::preset("float", Preset::Float(float)))
        .token(Token::preset("string", Preset::String(string)))
        .line_comment("//")
        .nested_block_comment("/*", "*/")
        .keywords(["return"])
        .contextual(["from"])
        .rule("File", many(field("items", r("Item"))))
        .rule("Item", choice![r("LetStmt"), r("FnDecl"), r("ExprStmt")])
        .rule(
            "LetStmt",
            seq![
                lit("let"),
                field("name", r("Name")),
                lit("="),
                field("value", r("Expr")),
                lit(";")
            ],
        )
        .rule(
            "FnDecl",
            seq![
                lit("fn"),
                field("name", r("Name")),
                lit("("),
                field("params", sep(r("Param"), ",")),
                lit(")"),
                field("body", r("Block"))
            ],
        )
        .rule("Param", field("name", r("Name")))
        .rule(
            "Block",
            seq![lit("{"), field("stmts", many(r("Item"))), lit("}")],
        )
        // The last statement of a block may leave its `;` out.
        .rule(
            "ExprStmt",
            seq![field("expr", r("Expr")), choice![lit(";"), look(lit("}"))]],
        )
        .rule("Name", r("ident"))
        .rule("NameRef", r("ident"))
        .expr(expressions())
        .describe("Item", "a statement")
        .describe("ident", "identifier")
        .build()
        .expect("the Calc grammar is fixed, and the tests build it")
}

/// The Calc language, built on first use.
///
/// ```
/// use langmeld_syntax::{FileId, SourceFile};
///
/// let file = SourceFile::new(FileId::new(0), "a.calc", "let x = 1 + 2;")?;
/// assert!(langmeld_syntax::parse(calc::language(), &file).diagnostics.is_empty());
/// # Ok::<(), langmeld_syntax::TextTooLarge>(())
/// ```
pub fn language() -> &'static Language {
    static LANGUAGE: OnceLock<Language> = OnceLock::new();
    LANGUAGE.get_or_init(build)
}
