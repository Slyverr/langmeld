//! Core types for building languages: source files, positions, diagnostics
//! and syntax tree kinds.
//!
//! - Files: [`FileId`] names a file, and [`SourceFile`] holds its name and text.
//! - Positions: [`TextRange`] and [`TextSize`] are byte offsets, and
//!   [`LineIndex`] converts them to lines and columns in UTF-8 bytes
//!   ([`LineCol`]) or UTF-16 code units ([`Utf16LineCol`]).
//! - Problems: [`Diagnostic`], with its [`Severity`], [`Label`], [`Fix`] and
//!   [`TextEdit`].
//! - Trees: [`SyntaxKind`] numbers the kinds of nodes and tokens, and
//!   [`SyntaxLanguage`] binds them to the `rowan` syntax tree library.
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
mod line_index;

pub use diagnostic::{Diagnostic, Fix, Label, Severity, TextEdit};
pub use file::{FileId, SourceFile};
pub use kind::{SyntaxKind, SyntaxLanguage};
pub use line_index::{LineCol, LineIndex, TextTooLarge, Utf16LineCol};
pub use text_size::{TextRange, TextSize};
