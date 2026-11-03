#![deny(missing_docs)]

//! Shared source contracts and diagnostics for Langmeld language tools.

mod diagnostic;
mod source;
mod spec;

pub use diagnostic::{Diagnostic, Severity};
pub use source::{SourceDatabase, SourceDatabaseError, SourceFile, SourceId, Span};
pub use spec::{
    BehaviorSpec, BehaviorStage, BlockSpec, CategorySpec, ConstructSpec, EnumSpec,
    FieldConstraints, FieldKind, FieldSpec, LanguageSpec, MapKeyKind, ScopeLookup, ScopeSpec,
    SymbolSpec, SyntaxRuleSpec, TypeSpec,
};
