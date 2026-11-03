//! Prepared-pattern compiler and shared parser internals.

mod candidates;
mod engine;
mod input;
mod matching;
mod pattern;
mod prepared;

pub use prepared::{
    ConfiguredLanguageParser, ParserRegistrationError, ParserRegistrationIssue,
    PatternCompileError, PreparedLanguage,
};
