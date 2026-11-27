//! Core types for building languages: source files, positions, diagnostics,
//! a lexer, parser primitives and syntax trees.
//!
//! - Files: [`FileId`] names a file, and [`SourceFile`] holds its name and text.
//! - Positions: [`TextRange`] and [`TextSize`] are byte offsets, and
//!   [`LineIndex`] converts them to lines and columns in UTF-8 bytes
//!   ([`LineCol`]) or UTF-16 code units ([`Utf16LineCol`]).
//! - Problems: [`Diagnostic`], with its [`Severity`], [`Label`], [`Fix`] and
//!   [`TextEdit`].
//! - Trees: [`SyntaxKind`] numbers the kinds of nodes and tokens, and
//!   [`SyntaxLanguage`] binds them to the `rowan` syntax tree library, as
//!   [`SyntaxNode`] and [`SyntaxToken`].
//! - Lexing: declare [`TokenSpec`]s, build a [`Lexer`] and get a [`Lexed`] list of
//!   [`Lexeme`]s that covers the file.
//! - Parsing: write functions that drive a [`Parser`], which builds a [`Parse`]
//!   (a lossless tree and diagnostics) through a [`TreeBuilder`].
//! - Grammars: declare a [`Language`] with [`Language::builder`] and the
//!   [`pattern`] functions, and [`parse`] files with it. The grammar is checked when it
//!   is built, and what is wrong is reported as [`Diagnostic`]s with a way to fix it.
//!
//! ```
//! use langmeld_syntax::{FileId, SourceFile, TextSize};
//!
//! let file = SourceFile::new(FileId::new(0), "main.calc", "let x = 1\nx")?;
//! let position = file.line_index().line_col(TextSize::new(10));
//! assert_eq!(position.map(|p| (p.line, p.col)), Some((1, 0)));
//! # Ok::<(), langmeld_syntax::TextTooLarge>(())
//! ```
//!
//! This crate builds for `wasm32-unknown-unknown`: it uses no threads,
//! filesystem, time or stdio.

mod diagnostic;
mod externals;
mod file;
mod grammar;
mod kind;
mod lexeme;
mod lexer;
mod line_index;
mod macros;
mod parser;

pub use diagnostic::{Diagnostic, Fix, Label, Severity, TextEdit};
pub use externals::Externals;
pub use file::{FileId, SourceFile};
pub use grammar::{
    Assoc, Cardinality, ExprSpec, ExternRule, FieldSchema, Language, LanguageBuilder, NodeSchema,
    Pattern, Schema, SchemaError, Token, parse, pattern,
};
pub use kind::{SyntaxKind, SyntaxLanguage, SyntaxNode, SyntaxToken};
pub use lexeme::Lexeme;
pub use lexer::{
    FloatPreset, IdentStyle, IntPreset, Interpolation, LexCursor, LexLimits, Lexed, Lexer,
    LexerBuilder, Preset, StringParts, StringPreset, Terminators, TokenSpec,
};
pub use line_index::{LineCol, LineIndex, TextTooLarge, Utf16LineCol};
pub use parser::{CompletedMarker, Marker, Parse, ParseLimits, Parser, TreeBuilder};
pub use rowan::GreenNode;
pub use text_size::{TextRange, TextSize};

/// The guide's code blocks, compiled and run as doc tests so they stay correct.
#[cfg(doctest)]
#[doc = include_str!("../../../docs/guides/hand-written-parser.md")]
struct HandWrittenParserGuide;

/// The guide's code blocks, compiled and run as doc tests so they stay correct.
#[cfg(doctest)]
#[doc = include_str!("../../../docs/guides/declare-a-grammar.md")]
struct DeclareAGrammarGuide;
