//! Binding, project loading, the language server, editor packaging and the helper kit.
//!
//! **Experimental:** the API may change in any minor release.
//!
//! - [`bind`]: names across the files of a project, as the `bind` block of a grammar
//!   declares them: what each use means, what is not defined, and renaming safely.
//! - [`project`]: where the files of a project come from, and the project file of a
//!   language's users.
//! - [`kit`]: help for what a language does with its trees.
//! - With the `lsp` feature, the language server: [`Server`] answers the messages of the
//!   Language Server Protocol, whatever carries them, and [`serve`] carries them on
//!   standard input and output. A language adds its own checks with [`LanguageHooks`].
//!
//! This crate builds for `wasm32-unknown-unknown`.

pub mod bind;
pub mod kit;
#[cfg(feature = "lsp")]
mod lsp;
pub mod project;
mod served;

pub use served::Served;

#[cfg(feature = "lsp")]
pub use lsp::{
    DEFAULT_LIMIT, LanguageHooks, Message, Server, StdioTransport, Transport, serve, serve_with,
};
