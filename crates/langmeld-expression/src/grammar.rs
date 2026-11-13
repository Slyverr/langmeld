//! Validated expression operator tables.

use std::collections::HashSet;

/// Associativity of a binary operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Associativity {
    /// Operators of equal precedence group from left to right.
    Left,
    /// Operators of equal precedence group from right to left.
    Right,
}

/// Position in which an operator may appear.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OperatorFixity {
    /// Operator appears before its operand.
    Prefix,
    /// Operator appears between two operands.
    Infix(Associativity),
}

/// One prefix or infix operator declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperatorSpec {
    pub(crate) token: String,
    pub(crate) precedence: u32,
    pub(crate) fixity: OperatorFixity,
}

impl OperatorSpec {
    /// Creates a prefix operator with the given binding power.
    #[must_use]
    pub fn prefix(token: impl Into<String>, precedence: u32) -> Self {
        Self {
            token: token.into(),
            precedence,
            fixity: OperatorFixity::Prefix,
        }
    }

    /// Creates an infix operator with precedence and associativity.
    #[must_use]
    pub fn infix(token: impl Into<String>, precedence: u32, associativity: Associativity) -> Self {
        Self {
            token: token.into(),
            precedence,
            fixity: OperatorFixity::Infix(associativity),
        }
    }

    /// Returns the operator token.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Returns this operator's precedence.
    #[must_use]
    pub const fn precedence(&self) -> u32 {
        self.precedence
    }

    /// Returns this operator's permitted position.
    #[must_use]
    pub const fn fixity(&self) -> OperatorFixity {
        self.fixity
    }
}

/// A validated set of operators for expression parsing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExpressionGrammar {
    pub(crate) operators: Vec<OperatorSpec>,
    pub(crate) operator_tokens: Vec<String>,
}

impl ExpressionGrammar {
    /// Validates and creates an expression grammar.
    ///
    /// # Errors
    ///
    /// Returns [`GrammarError`] when an operator is empty, uses reserved
    /// syntax, has zero precedence, or duplicates another operator with the
    /// same fixity.
    pub fn new(operators: impl IntoIterator<Item = OperatorSpec>) -> Result<Self, GrammarError> {
        let operators: Vec<_> = operators.into_iter().collect();
        let mut fixities = HashSet::new();
        let mut tokens = Vec::new();
        for operator in &operators {
            if operator.token.is_empty()
                || !operator
                    .token
                    .chars()
                    .all(|character| character.is_ascii_punctuation())
                || operator.token.chars().any(|character| {
                    matches!(character, '"' | '.' | ',' | '(' | ')' | '[' | ']' | ';')
                })
            {
                return Err(GrammarError::InvalidOperator(operator.token.clone()));
            }
            if operator.precedence == 0 {
                return Err(GrammarError::ZeroPrecedence(operator.token.clone()));
            }
            let is_prefix = operator.fixity == OperatorFixity::Prefix;
            if !fixities.insert((operator.token.as_str(), is_prefix)) {
                return Err(GrammarError::DuplicateOperator {
                    token: operator.token.clone(),
                    fixity: operator.fixity,
                });
            }
            if !tokens.contains(&operator.token) {
                tokens.push(operator.token.clone());
            }
        }
        tokens.sort_by_key(|token| std::cmp::Reverse(token.len()));
        Ok(Self {
            operators,
            operator_tokens: tokens,
        })
    }

    /// Returns the declared prefix and infix operators.
    #[must_use]
    pub fn operators(&self) -> &[OperatorSpec] {
        &self.operators
    }

    pub(crate) fn operator(&self, token: &str, fixity: OperatorFixity) -> Option<&OperatorSpec> {
        self.operators
            .iter()
            .find(|operator| operator.token == token && operator.fixity == fixity)
    }
}

/// An invalid expression operator table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GrammarError {
    /// Operator text is empty or conflicts with built-in delimiters.
    InvalidOperator(String),
    /// Precedence must be greater than zero.
    ZeroPrecedence(String),
    /// The same operator and fixity are declared more than once.
    DuplicateOperator {
        /// Duplicated operator token.
        token: String,
        /// Duplicated operator position.
        fixity: OperatorFixity,
    },
}

impl std::fmt::Display for GrammarError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOperator(token) => {
                write!(
                    formatter,
                    "operator `{token}` is empty or uses reserved syntax"
                )
            }
            Self::ZeroPrecedence(token) => {
                write!(formatter, "operator `{token}` must have nonzero precedence")
            }
            Self::DuplicateOperator { token, fixity } => {
                write!(
                    formatter,
                    "operator `{token}` repeats the {fixity:?} declaration"
                )
            }
        }
    }
}

impl std::error::Error for GrammarError {}
