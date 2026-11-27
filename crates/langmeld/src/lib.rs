//! Langmeld is an experimental Rust framework for building languages.
//!
//! This facade is the only crate users depend on. A language is declared in a `.langmeld`
//! file, read with [`load_language`], and run with [`parse`]:
//!
//! ```
//! use langmeld::{FileId, Source, SourceFile, load_language, parse};
//!
//! let loaded = load_language(Source::text(
//!     "greeting.langmeld",
//!     r#"langmeld 1
//! language Greeting { extensions "hi" }
//! tokens { skip space = /[ \t\n]+/
//!          name = preset ident(ascii) }
//! rule File = Greeting*
//! rule Greeting = "hello" who:name
//! "#,
//! ))?;
//! let file = SourceFile::new(FileId::new(0), "a.hi", "hello world hello rust")?;
//! let parsed = parse(&loaded.language, &file);
//! assert!(parsed.diagnostics.is_empty());
//! assert_eq!(parsed.syntax().children().count(), 2);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! The same language can be declared in Rust with [`Language::builder`], or compiled when
//! the program is built with [`build::embed`] and read with [`Source::embedded`].
//! Everything the syntax crate offers is also at [`syntax`].

pub use langmeld_meta::{
    Bins, Layout, LoadError, Loaded, MANIFEST_NAME, Manifest, Package, Project, Source, VsCode,
    build, load_language,
};
pub use langmeld_syntax as syntax;
pub use langmeld_syntax::{
    Assoc, Cardinality, CompletedMarker, Diagnostic, EmbedError, ExprSpec, ExternRule, Externals,
    FieldSchema, FileId, Fix, FloatPreset, GreenNode, IdentStyle, IntPreset, Interpolation, Label,
    Language, LanguageBuilder, LexCursor, LexLimits, Lexed, Lexeme, Lexer, LexerBuilder, LineCol,
    LineIndex, Lookahead, Marker, NodeSchema, Parse, ParseLimits, Parser, Pattern, Preset, Schema,
    SchemaError, Severity, SourceFile, StringParts, StringPreset, SyntaxKind, SyntaxLanguage,
    SyntaxNode, SyntaxToken, Terminators, TextEdit, TextRange, TextSize, TextTooLarge, Token,
    TokenSpec, TreeBuilder, Utf16LineCol, parse, pattern,
};

/// The guide's code blocks, compiled and run as doc tests so they stay correct.
#[cfg(doctest)]
#[doc = include_str!("../../../docs/guides/write-a-langmeld-file.md")]
struct WriteALangmeldFileGuide;
