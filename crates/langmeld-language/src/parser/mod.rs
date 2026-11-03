//! Prepared-pattern compiler and shared parser internals.

mod candidates;
#[allow(dead_code)]
mod matching;
mod pattern;
mod prepared;

pub use prepared::{PatternCompileError, PreparedLanguage};
