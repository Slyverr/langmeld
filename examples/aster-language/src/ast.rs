use langmeld_core::Span;
use langmeld_expression::Expression;

/// Aster's host-owned syntax tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Program {
    /// Function declarations in source order.
    pub functions: Vec<Function>,
}

/// A function declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    /// Function name.
    pub name: String,
    /// Source range of the declared name.
    pub name_span: Span,
    /// Parameters in source order.
    pub parameters: Vec<Parameter>,
    /// Declared return type.
    pub return_type: ValueType,
    /// Function body statements.
    pub body: Vec<Statement>,
    /// Complete source range of the declaration.
    pub span: Span,
}

/// A function parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    /// Parameter name.
    pub name: String,
    /// Parameter type.
    pub value_type: ValueType,
    /// Source range of the parameter name.
    pub name_span: Span,
}

/// Aster's built-in value types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueType {
    /// Signed 32-bit integer.
    I32,
    /// Boolean value.
    Bool,
    /// UTF-8 string literal value.
    String,
    /// Type could not be determined because of an earlier error.
    Error,
}

/// A statement in a function body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    /// Declare a local whose type is inferred from its initializer.
    Let {
        /// Declared local name.
        name: String,
        /// Source range of the local name.
        name_span: Span,
        /// Initializer expression.
        value: Expression,
        /// Complete source range of the declaration.
        span: Span,
    },
    /// Return an expression from the current function.
    Return {
        /// Returned expression.
        value: Expression,
        /// Complete source range of the return statement.
        span: Span,
    },
    /// A lexical block that introduces a nested scope.
    Block {
        /// Child statements.
        statements: Vec<Statement>,
        /// Complete source range including delimiters.
        span: Span,
    },
}

impl std::fmt::Display for ValueType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::I32 => formatter.write_str("i32"),
            Self::Bool => formatter.write_str("bool"),
            Self::String => formatter.write_str("string"),
            Self::Error => formatter.write_str("<error>"),
        }
    }
}
