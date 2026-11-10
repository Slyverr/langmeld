//! A small host-owned language parser and analyzer built with Langmeld.

mod analysis;
mod ast;
mod parser;

pub use analysis::{
    Analysis, NameReference, ScopeId, Symbol, SymbolId, SymbolKind, SymbolType, analyze,
};
pub use ast::{Function, Parameter, Program, Statement, ValueType};
pub use parser::{AsterParser, parse_program};
