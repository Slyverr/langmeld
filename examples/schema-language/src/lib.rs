//! A small schema language with host-owned parsing and global type resolution.

mod analysis;
mod ast;
mod completion;
mod navigation;
mod parser;

pub use analysis::{
    Analysis, RecordSymbol, SchemaAnalyzer, SchemaWorkspaceAnalyzer, TypeReference, analyze,
};
pub use ast::{Field, Record, Schema, TypeName};
pub use completion::SchemaCompleter;
pub use navigation::SchemaNavigator;
pub use parser::{SchemaParser, parse_schema};
