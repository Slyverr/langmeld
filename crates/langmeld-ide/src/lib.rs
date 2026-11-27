//! Binding, project loading, the language server, editor packaging and the helper kit.
//!
//! **Experimental:** the API may change in any minor release.
//!
//! - [`kit`]: help for what a language does with its trees.
//! - With the `lsp` feature, the language server: [`Server`] answers the messages of the
//!   Language Server Protocol, whatever carries them, and [`serve`] carries them on
//!   standard input and output. A language adds its own checks with [`LanguageHooks`].
//!
//! This crate builds for `wasm32-unknown-unknown`.

pub mod kit;
#[cfg(feature = "lsp")]
mod lsp;
mod served;

pub use served::Served;

#[cfg(feature = "lsp")]
pub use lsp::{
    DEFAULT_LIMIT, LanguageHooks, Message, Server, StdioTransport, Transport, serve, serve_with,
};
