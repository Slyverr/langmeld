//! Lexical TextMate grammar generation from Langmeld declarative languages.
#![deny(missing_docs)]

mod error;
mod generator;
mod model;
mod patterns;

pub use error::TextMateError;
pub use generator::{TextMateOptions, generate};
