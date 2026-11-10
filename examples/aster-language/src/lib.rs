//! A small host-owned language parser and analyzer built with Langmeld.

mod analysis;
mod ast;
mod navigation;
mod parser;

pub use analysis::{
    Analysis, AsterAnalyzer, NameReference, ScopeId, Symbol, SymbolId, SymbolKind, SymbolType,
    analyze,
};
pub use ast::{Function, Parameter, Program, Statement, ValueType};
pub use navigation::AsterNavigator;
pub use parser::{AsterParser, parse_program};
