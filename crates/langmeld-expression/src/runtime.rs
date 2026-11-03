use langmeld_core::{Diagnostic, Span};

use crate::{Atom, Expression, MAX_EXPRESSION_NESTING};

/// Host-defined runtime behavior for parsed expressions.
pub trait ExpressionRuntime {
    /// Host-defined runtime value.
    type Value;

    /// Evaluates a literal or identifier.
    fn atom(&mut self, atom: &Atom, span: Span) -> Result<Self::Value, String>;

    /// Evaluates a prefix operation.
    fn prefix(
        &mut self,
        operator: &str,
        operator_span: Span,
        operand: &Self::Value,
    ) -> Result<Self::Value, String>;

    /// Evaluates an infix operation after both operands are evaluated.
    fn infix(
        &mut self,
        operator: &str,
        operator_span: Span,
        left: &Self::Value,
        right: &Self::Value,
    ) -> Result<Self::Value, String>;

    /// Evaluates a call after evaluating its callee and arguments from left to right.
    /// The default rejects calls for runtimes that do not define callable values.
    fn call(
        &mut self,
        _callee: &Self::Value,
        _arguments: &[Self::Value],
        _call_span: Span,
    ) -> Result<Self::Value, String> {
        Err("function calls are not supported by this language".to_owned())
    }

    /// Evaluates a named member access on a host value.
    fn member(
        &mut self,
        _receiver: &Self::Value,
        _name: &str,
        _member_span: Span,
    ) -> Result<Self::Value, String> {
        Err("member access is not supported by this language".to_owned())
    }

    /// Evaluates indexed access on a host value.
    fn index(
        &mut self,
        _receiver: &Self::Value,
        _index: &Self::Value,
        _index_span: Span,
    ) -> Result<Self::Value, String> {
        Err("indexed access is not supported by this language".to_owned())
    }

    /// Returns a result when the left operand completes a short-circuit operator.
    /// Returning `None` asks the evaluator to evaluate the right operand normally.
    fn short_circuit(&mut self, _operator: &str, _left: &Self::Value) -> Option<Self::Value> {
        None
    }
}

/// Evaluates a parsed expression with host-defined runtime behavior.
pub fn evaluate_expression<R: ExpressionRuntime>(
    expression: &Expression,
    runtime: &mut R,
) -> Result<R::Value, Diagnostic> {
    evaluate(expression, runtime, 0)
        .map_err(|(span, message)| Diagnostic::error("LE3001", message, span))
}

fn evaluate<R: ExpressionRuntime>(
    expression: &Expression,
    runtime: &mut R,
    depth: usize,
) -> Result<R::Value, (Span, String)> {
    if depth > MAX_EXPRESSION_NESTING {
        return Err((
            expression.span(),
            format!("expression exceeds the {MAX_EXPRESSION_NESTING}-level nesting limit"),
        ));
    }
    match expression {
        Expression::Atom(atom, span) => runtime
            .atom(atom, *span)
            .map_err(|message| (*span, message)),
        Expression::Group { expression, .. } => evaluate(expression, runtime, depth + 1),
        Expression::Prefix {
            operator,
            operator_span,
            operand,
            ..
        } => {
            let operand = evaluate(operand, runtime, depth + 1)?;
            runtime
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
            let left = evaluate(left, runtime, depth + 1)?;
            if let Some(value) = runtime.short_circuit(operator, &left) {
                return Ok(value);
            }
            let right = evaluate(right, runtime, depth + 1)?;
            runtime
                .infix(operator, *operator_span, &left, &right)
                .map_err(|message| (*operator_span, message))
        }
        Expression::Call {
            callee,
            arguments,
            span,
        } => {
            let callee = evaluate(callee, runtime, depth + 1)?;
            let arguments = arguments
                .iter()
                .map(|argument| evaluate(argument, runtime, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            runtime
                .call(&callee, &arguments, *span)
                .map_err(|message| (*span, message))
        }
        Expression::Member {
            receiver,
            name,
            name_span,
            ..
        } => {
            let receiver = evaluate(receiver, runtime, depth + 1)?;
            runtime
                .member(&receiver, name, *name_span)
                .map_err(|message| (*name_span, message))
        }
        Expression::Index {
            receiver,
            index,
            span,
        } => {
            let receiver = evaluate(receiver, runtime, depth + 1)?;
            let index = evaluate(index, runtime, depth + 1)?;
            runtime
                .index(&receiver, &index, *span)
                .map_err(|message| (*span, message))
        }
    }
}
