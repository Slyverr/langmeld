//! Calc, a small calculator language declared in `calc.langmeld`, with an interpreter and
//! a command line. The same language is also declared with the grammar builder, as
//! [`language`], and written by hand in the `handwritten-calc` example; tests hold the
//! three to the same trees.
//!
//! - [`syntax`]: the types of the trees, written from the grammar when the example is
//!   built.
//! - [`interp`]: what programs mean.
//! - [`cli`]: the `calc` program, `calc run file.calc` or `calc eval "1 + 2 * 3"`.

pub mod checks;
#[cfg(feature = "cli")]
pub mod cli;
pub mod interp;

use std::sync::OnceLock;

use langmeld::bind::Resolver;

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
        .rule(
            "Item",
            choice![r("UseStmt"), r("LetStmt"), r("FnDecl"), r("ExprStmt")],
        )
        .rule(
            "UseStmt",
            seq![lit("use"), field("path", r("string")), lit(";")],
        )
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

/// What Calc knows without being told: the functions `print`, `sqrt` and `abs`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Builtins;

impl Resolver for Builtins {
    fn predefined(&self) -> &[(&str, &str)] {
        &[("value", "print"), ("value", "sqrt"), ("value", "abs")]
    }
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

/// The types of Calc's syntax trees, written from `calc.langmeld` when the example was
/// built, and the language itself.
pub mod syntax {
    langmeld::include_syntax!();
}
