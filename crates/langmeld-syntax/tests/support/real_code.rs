//! The grammar of each file of real-code tree cases that Calc does not cover: as small as
//! the case needs. The other files run against the Calc example, in `examples/calc` and
//! `examples/handwritten-calc`.

use langmeld_syntax::pattern::{choice, field, lit, opt, r, sep};
use langmeld_syntax::{
    Assoc, ExprSpec, FileId, FloatPreset, IdentStyle, IntPreset, Language, LanguageBuilder, Preset,
    SourceFile, Token, parse, seq,
};

fn basics(name: &str) -> LanguageBuilder {
    let mut int = IntPreset::default();
    int.hex = true;
    int.underscores = true;
    let mut float = FloatPreset::default();
    float.exponent = true;
    Language::builder(name)
        .token(Token::skip("space", r"[ \t\r\n]+"))
        .token(Token::preset("ident", Preset::Ident(IdentStyle::Xid)))
        .token(Token::preset("int", Preset::Int(int)))
        .token(Token::preset("float", Preset::Float(float)))
        .token(Token::preset("char", Preset::Char))
}

fn expressions() -> ExprSpec {
    ExprSpec::new("Expr")
        .atom("Literal", r("int"))
        .atom("Var", r("NameRef"))
        .atom("Paren", seq![lit("("), r("Expr"), lit(")")])
        .prefix("Neg", ["-"], 9)
        .infix("Shift", [">>"], 7, Assoc::Left)
        .infix("Mul", ["*"], 6, Assoc::Left)
        .infix("Add", ["+"], 5, Assoc::Left)
        .infix("Compare", ["!=", ">"], 4, Assoc::None)
        .infix("Assign", ["="], 1, Assoc::Right)
        .postfix("Call", seq![lit("("), sep(r("Expr"), ","), lit(")")], 10)
}

fn language(file: &str) -> Option<Language> {
    let builder = match file {
        "operators.txt" => basics("Operators")
            .rule("File", r("Expr"))
            .rule("NameRef", r("ident"))
            .expr(expressions()),
        "generics.txt" => basics("Generics")
            .rule("File", r("Type"))
            .rule(
                "Type",
                seq![
                    field("name", r("NameRef")),
                    opt(seq![lit("<"), sep(r("Type"), ","), lit(">")])
                ],
            )
            .rule("NameRef", r("ident")),
        "characters.txt" => basics("Characters")
            .rule("File", r("Literal"))
            .rule("Literal", choice([r("int"), r("float"), r("char")])),
        _ => return None,
    };
    Some(builder.build().expect("the grammar is valid"))
}

/// Whether the files of `file` have a grammar here.
pub fn covers(file: &str) -> bool {
    matches!(file, "operators.txt" | "generics.txt" | "characters.txt")
}

pub fn parse_tree(file: &str, source: &str) -> Option<String> {
    let language = language(file)?;
    let file = SourceFile::new(FileId::new(0), file, source).expect("small");
    let parsed = parse(&language, &file);
    Some(crate::render::render(
        &parsed,
        &|kind| language.kind_name(kind).to_owned(),
        &|kind| language.is_trivia(kind),
    ))
}
