use langmeld_core::Span;
use langmeld_expression::{
    Associativity, Expression, ExpressionGrammar, ExpressionKind, ExpressionParser, OperatorSpec,
};
use langmeld_language::{ConstructContext, ConstructHandler, HandlerControl, ParsedConstruct};

use crate::runtime::RuntimeState;

pub(super) struct ComputeHandler;

impl ConstructHandler<RuntimeState> for ComputeHandler {
    const CONSTRUCT: &'static str = "compute";

    fn handle(
        &self,
        construct: &ParsedConstruct,
        context: &mut ConstructContext<'_, RuntimeState>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        let expression_field = context.field(construct, "value")?;
        let grammar = ExpressionGrammar::new([
            OperatorSpec::infix("+", 1, Associativity::Left),
            OperatorSpec::infix("*", 2, Associativity::Left),
        ])
        .map_err(|error| context.error_at("TOY-003", error.to_string(), expression_field.span))?;
        let parsed = ExpressionParser::new(&grammar).parse(context.source(), expression_field.span);

        if let Some(diagnostic) = parsed.diagnostics.into_iter().next() {
            return Err(diagnostic);
        }

        let expression = parsed.expression.ok_or_else(|| {
            context.error_at(
                "TOY-003",
                "expected an arithmetic expression",
                expression_field.span,
            )
        })?;
        let value = evaluate(&expression)
            .map_err(|(message, span)| context.error_at("TOY-004", message, span))?;

        context.state_mut().output.push_str(&format!("{value}\n"));

        Ok(HandlerControl::Continue)
    }
}

fn evaluate(expression: &Expression) -> Result<i64, (String, Span)> {
    match &expression.kind {
        ExpressionKind::Integer(value) => value
            .parse()
            .map_err(|error| (format!("integer is out of range: {error}"), expression.span)),
        ExpressionKind::Group(expression) => evaluate(expression),
        ExpressionKind::Infix {
            operator,
            left,
            right,
        } => {
            let left_value = evaluate(left)?;
            let right_value = evaluate(right)?;
            let result = match operator.as_str() {
                "+" => left_value.checked_add(right_value),
                "*" => left_value.checked_mul(right_value),
                _ => {
                    return Err((
                        format!("unsupported arithmetic operator `{operator}`"),
                        expression.span,
                    ));
                }
            };

            result.ok_or_else(|| ("integer arithmetic overflow".to_owned(), expression.span))
        }
        _ => Err((
            "this example evaluates only integer addition and multiplication".to_owned(),
            expression.span,
        )),
    }
}
