//! What Calc checks beyond its grammar. `calc check` and the language server both show it.

use langmeld::ast::AstNode;
use langmeld::{Diagnostic, Parse, SourceFile};

use crate::syntax::{Expr, Mul};

/// Warns about divisions by a literal zero, which can only fail.
pub fn divisions_by_zero(file: &SourceFile, parsed: &Parse) -> Vec<Diagnostic> {
    parsed
        .syntax()
        .descendants()
        .filter_map(Mul::cast)
        .filter(|product| product.operator().is_some_and(|op| op.text() == "/"))
        .filter(|product| {
            matches!(product.rhs(), Some(Expr::Literal(zero)) if zero.syntax().text() == "0")
        })
        .map(|product| {
            Diagnostic::warning(
                "CALC010",
                file.id(),
                product.range(),
                "this divides by zero",
            )
            .note("To fix this, divide by a number that is not zero.")
        })
        .collect()
}
