//! A small schema language with host-owned parsing and global type resolution.

mod analysis;
mod ast;
mod parser;

pub use analysis::{Analysis, RecordSymbol, SchemaAnalyzer, TypeReference, analyze};
pub use ast::{Field, Record, Schema, TypeName};
pub use parser::{SchemaParser, parse_schema};
