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
mod file;
mod kind;
mod lexeme;
mod lexer;
mod line_index;
mod parser;

pub use diagnostic::{Diagnostic, Fix, Label, Severity, TextEdit};
pub use file::{FileId, SourceFile};
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
