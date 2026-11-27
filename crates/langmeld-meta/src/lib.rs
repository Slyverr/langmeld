//! The `.langmeld` format: reading files into languages, `langmeld.toml`, and the corpus
//! files that test languages.
//!
//! **Experimental:** the API may change in any minor release.
//!
//! [`load_language`] reads a `.langmeld` file and what it imports, checks them, and
//! gives a [`Language`](langmeld_syntax::Language) that parses like one declared with the
//! builder. Every mistake in a file is a diagnostic at its place in that file.

pub mod build;
pub mod codegen;
pub mod corpus;
mod declared;
mod load;
mod lower;
mod manifest;
mod meta;
mod problem;
mod render;
mod tree;

pub use declared::{Binding, Bindings, Editor, EditorRule, Path, Place};
#[cfg(not(target_arch = "wasm32"))]
pub use load::DiskFiles;
pub use load::{Files, LoadError, Loaded, MemoryFiles, Source, load_language, render_warnings};
pub use manifest::{
    Bins, DEFAULT_ROOT, FILE_NAME as MANIFEST_NAME, Layout, Manifest, Package, Project, VsCode,
};
pub use render::{place, render};
