use langmeld_core::{Diagnostic, Span};

use crate::{Atom, Expression, MAX_EXPRESSION_NESTING};

/// Host rules for assigning types to literals, variables, and operators.
pub trait ExpressionTypeRules {
    /// Host-defined static type.
    type Type: Clone;

    /// Infers a literal or identifier's type.
    fn atom(&mut self, atom: &Atom, span: Span) -> Result<Self::Type, String>;

    /// Infers the result type of a prefix operation.
    fn prefix(
        &mut self,
        operator: &str,
        operator_span: Span,
        operand: &Self::Type,
    ) -> Result<Self::Type, String>;

    /// Infers the result type of an infix operation.
    fn infix(
        &mut self,
        operator: &str,
        operator_span: Span,
        left: &Self::Type,
        right: &Self::Type,
    ) -> Result<Self::Type, String>;

    /// Infers the result type of a call. Hosts can model function signatures or
    /// dynamic call behavior; the default rejects calls for languages that omit them.
    fn call(
        &mut self,
        _callee: &Self::Type,
        _arguments: &[Self::Type],
        _call_span: Span,
    ) -> Result<Self::Type, String> {
        Err("function calls are not supported by this language".to_owned())
    }

    /// Infers the result type of a named member access.
    fn member(
        &mut self,
        _receiver: &Self::Type,
        _name: &str,
        _member_span: Span,
    ) -> Result<Self::Type, String> {
        Err("member access is not supported by this language".to_owned())
    }

    /// Infers the result type of indexed access.
    fn index(
        &mut self,
        _receiver: &Self::Type,
        _index: &Self::Type,
        _index_span: Span,
    ) -> Result<Self::Type, String> {
        Err("indexed access is not supported by this language".to_owned())
    }
}

/// Applies host-provided type rules to a parsed expression.
pub fn infer_expression_type<R: ExpressionTypeRules>(
    expression: &Expression,
    rules: &mut R,
) -> Result<R::Type, Vec<Diagnostic>> {
    infer(expression, rules, 0)
        .map_err(|(span, message)| vec![Diagnostic::error("LE2001", message, span)])
}

fn infer<R: ExpressionTypeRules>(
    expression: &Expression,
    rules: &mut R,
    depth: usize,
) -> Result<R::Type, (Span, String)> {
    if depth > MAX_EXPRESSION_NESTING {
        return Err((
            expression.span(),
            format!("expression exceeds the {MAX_EXPRESSION_NESTING}-level nesting limit"),
        ));
    }
    match expression {
        Expression::Atom(atom, span) => rules.atom(atom, *span).map_err(|message| (*span, message)),
        Expression::Group { expression, .. } => infer(expression, rules, depth + 1),
        Expression::Prefix {
            operator,
            operator_span,
            operand,
            ..
        } => {
            let operand = infer(operand, rules, depth + 1)?;
            rules
                .prefix(operator, *operator_span, &operand)
                .map_err(|message| (*operator_span, message))
        }
        Expression::Infix {
            left,
            operator,
            operator_span,
            right,
            ..
        } => {
            let left_type = infer(left, rules, depth + 1)?;
            let right_type = infer(right, rules, depth + 1)?;
            rules
                .infix(operator, *operator_span, &left_type, &right_type)
                .map_err(|message| (*operator_span, message))
        }
        Expression::Call {
            callee,
            arguments,
            span,
        } => {
            let callee = infer(callee, rules, depth + 1)?;
            let arguments = arguments
                .iter()
                .map(|argument| infer(argument, rules, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            rules
                .call(&callee, &arguments, *span)
                .map_err(|message| (*span, message))
        }
        Expression::Member {
            receiver,
            name,
            name_span,
            ..
        } => {
            let receiver = infer(receiver, rules, depth + 1)?;
            rules
                .member(&receiver, name, *name_span)
                .map_err(|message| (*name_span, message))
        }
        Expression::Index {
            receiver,
            index,
            span,
        } => {
            let receiver = infer(receiver, rules, depth + 1)?;
            let index = infer(index, rules, depth + 1)?;
            rules
                .index(&receiver, &index, *span)
                .map_err(|message| (*span, message))
        }
    }
}
