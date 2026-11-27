//! The facade offers what the chapters so far add, and nothing else: this file fails to
//! compile when one of them goes missing, and its list is the one to extend by a chapter.

#![allow(unused_imports)]

use langmeld::pattern::{
    attempt, choice, field, lit, look, many, many1, not, opt, r, sep, sep_strict, seq, terminated,
};
use langmeld::{
    Assoc, Bins, Cardinality, CompletedMarker, Diagnostic, EmbedError, ExprSpec, ExternRule,
    Externals, FieldSchema, FileId, Fix, FloatPreset, GreenNode, IdentStyle, IntPreset,
    Interpolation, Label, Language, LanguageBuilder, Layout, LexCursor, LexLimits, Lexed, Lexeme,
    Lexer, LexerBuilder, LineCol, LineIndex, LoadError, Loaded, Lookahead, MANIFEST_NAME, Manifest,
    Marker, NodeSchema, Package, Parse, ParseLimits, Parser, Pattern, Preset, Project, Schema,
    SchemaError, Severity, Source, SourceFile, StringParts, StringPreset, SyntaxKind,
    SyntaxLanguage, SyntaxNode, SyntaxToken, Terminators, TextEdit, TextRange, TextSize,
    TextTooLarge, Token, TokenSpec, TreeBuilder, Utf16LineCol, VsCode, build, load_language, parse,
};

#[test]
fn a_language_can_be_declared_in_a_file_and_in_rust_and_gives_the_same_tree() {
    let from_file = load_language(Source::text(
        "bang.langmeld",
        "langmeld 1\nlanguage Bang { }\ntokens { skip space = /[ ]+/ }\nrule File = \"!\"*\n",
    ))
    .expect("valid");
    let from_rust = Language::builder("Bang")
        .token(Token::skip("space", "[ ]+"))
        .rule("File", many(lit("!")))
        .build()
        .expect("valid");
    let file = SourceFile::new(FileId::new(0), "x", "! ! ?").expect("small");
    let (a, b) = (parse(&from_file.language, &file), parse(&from_rust, &file));
    assert_eq!(format!("{:#?}", a.syntax()), format!("{:#?}", b.syntax()));
    assert_eq!(a.diagnostics.len(), b.diagnostics.len());
}

#[test]
fn the_syntax_crate_is_reachable_whole() {
    let _ = langmeld::syntax::SourceFile::new(FileId::new(0), "x", "").expect("small");
}
