//! Configurable expression parsing with host-owned operator meaning.
#![deny(missing_docs)]

mod grammar;
mod parser;

pub use grammar::{Associativity, ExpressionGrammar, GrammarError, OperatorFixity, OperatorSpec};
pub use parser::{
    Expression, ExpressionKind, ExpressionLimits, ExpressionParseResult,
    ExpressionParseResultParts, ExpressionParser, MAX_EXPRESSION_NESTING_DEPTH,
};
