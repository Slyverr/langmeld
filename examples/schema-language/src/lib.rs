//! A small schema language with host-owned parsing and global type resolution.

mod analysis;
mod ast;
mod completion;
mod parser;

pub use analysis::{Analysis, RecordSymbol, SchemaAnalyzer, TypeReference, analyze};
pub use ast::{Field, Record, Schema, TypeName};
pub use completion::SchemaCompleter;
pub use parser::{SchemaParser, parse_schema};
