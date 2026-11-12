//! Tree-sitter grammar generation from declarative Langmeld languages.
#![deny(missing_docs)]

mod error;
mod generator;

pub use error::TreeSitterError;
pub use generator::{TreeSitterOptions, generate};
