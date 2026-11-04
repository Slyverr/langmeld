#![deny(missing_docs)]

//! Shared language models, source contracts, and diagnostics for Langmeld.

mod builder;
mod diagnostic;
mod source;
mod spec;
mod validation;

pub use builder::{ConstructBuilder, LanguageBuilder, NamedEnum, NamedRecord, SyntaxPattern};
pub use diagnostic::{Diagnostic, Severity};
pub use source::{SourceDatabase, SourceDatabaseError, SourceFile, SourceId, Span};
pub use spec::{
    BehaviorSpec, BehaviorStage, BlockCommentSpec, BlockSeparator, BlockSeparatorPolicy, BlockSpec,
    CategorySpec, ConstructSpec, EnumSpec, ExpressionAssociativity, ExpressionAtomKind,
    ExpressionOperatorSpec, ExpressionSpec, FieldConstraints, FieldKind, FieldSpec,
    InfixOperatorSpec, LanguageSpec, LexicalSpec, MapKeyKind, ScopeLookup, ScopeSpec, SpecError,
    SymbolSpec, SyntaxRuleSpec, TypeSpec, ValidatedLanguageSpec,
};
