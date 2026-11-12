use std::sync::OnceLock;

use langmeld_expression::{Associativity, ExpressionGrammar, OperatorSpec};

pub(super) fn expression_grammar() -> &'static ExpressionGrammar {
    static GRAMMAR: OnceLock<ExpressionGrammar> = OnceLock::new();
    GRAMMAR.get_or_init(|| {
        ExpressionGrammar::new([
            OperatorSpec::infix("==", 1, Associativity::Left),
            OperatorSpec::infix("<", 1, Associativity::Left),
            OperatorSpec::infix("+", 2, Associativity::Left),
            OperatorSpec::infix("-", 2, Associativity::Left),
            OperatorSpec::infix("*", 3, Associativity::Left),
            OperatorSpec::infix("/", 3, Associativity::Left),
            OperatorSpec::prefix("-", 4),
        ])
        .expect("Aster's built-in expression grammar must remain valid")
    })
}
