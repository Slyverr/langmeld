use langmeld_core::Span;
use langmeld_expression::{Expression, ExpressionKind};

use crate::ValueType;

use super::{Analyzer, NameReference, ScopeId, SymbolType};

impl<'a> Analyzer<'a> {
    pub(super) fn expression_type(&mut self, expression: &Expression, scope: ScopeId) -> ValueType {
        match &expression.kind {
            ExpressionKind::Integer(_) => ValueType::I32,
            ExpressionKind::String(_) => ValueType::String,
            ExpressionKind::Name(name) => {
                let value_target = self.lookup_value(scope, name);
                let function_target = self.functions.get(name).copied();
                let target = value_target.or(function_target);
                self.analysis.references.push(NameReference {
                    name: name.clone(),
                    span: expression.span,
                    target,
                });
                if let Some(target) = value_target {
                    self.symbol_type(target)
                } else if function_target.is_some() {
                    self.error(
                        "ASTER-SEM-002",
                        format!("function `{name}` must be called"),
                        expression.span,
                    );
                    ValueType::Error
                } else {
                    self.error(
                        "ASTER-SEM-002",
                        format!("unknown name `{name}`"),
                        expression.span,
                    );
                    ValueType::Error
                }
            }
            ExpressionKind::Group(inner) => self.expression_type(inner, scope),
            ExpressionKind::Prefix { operator, operand } => {
                let operand_type = self.expression_type(operand, scope);
                if operator == "-" && operand_type == ValueType::I32 {
                    ValueType::I32
                } else {
                    self.expect_type(
                        ValueType::I32,
                        operand_type,
                        operand.span,
                        "unary arithmetic operand",
                    );
                    ValueType::Error
                }
            }
            ExpressionKind::Infix {
                operator,
                left,
                right,
            } => {
                let left_type = self.expression_type(left, scope);
                let right_type = self.expression_type(right, scope);
                match operator.as_str() {
                    "+" | "-" | "*" | "/" => {
                        self.expect_type(ValueType::I32, left_type, left.span, "left operand");
                        self.expect_type(ValueType::I32, right_type, right.span, "right operand");
                        if left_type == ValueType::I32 && right_type == ValueType::I32 {
                            ValueType::I32
                        } else {
                            ValueType::Error
                        }
                    }
                    "<" => {
                        self.expect_type(ValueType::I32, left_type, left.span, "left operand");
                        self.expect_type(ValueType::I32, right_type, right.span, "right operand");
                        if left_type == ValueType::I32 && right_type == ValueType::I32 {
                            ValueType::Bool
                        } else {
                            ValueType::Error
                        }
                    }
                    "==" => {
                        if left_type == ValueType::Error || right_type == ValueType::Error {
                            ValueType::Error
                        } else if left_type == right_type {
                            ValueType::Bool
                        } else {
                            self.error(
                                "ASTER-SEM-003",
                                "equality operands must have the same type",
                                expression.span,
                            );
                            ValueType::Error
                        }
                    }
                    _ => {
                        self.error(
                            "ASTER-SEM-004",
                            format!("operator `{operator}` is not supported by Aster"),
                            expression.span,
                        );
                        ValueType::Error
                    }
                }
            }
            ExpressionKind::Call { callee, arguments } => {
                self.call_type(callee, arguments, expression.span, scope)
            }
            ExpressionKind::Member { target, .. } => {
                let _ = self.expression_type(target, scope);
                self.error(
                    "ASTER-SEM-005",
                    "member access is not supported by Aster",
                    expression.span,
                );
                ValueType::Error
            }
            ExpressionKind::Index { target, index } => {
                let _ = self.expression_type(target, scope);
                let _ = self.expression_type(index, scope);
                self.error(
                    "ASTER-SEM-006",
                    "indexing is not supported by Aster",
                    expression.span,
                );
                ValueType::Error
            }
        }
    }

    fn call_type(
        &mut self,
        callee: &Expression,
        arguments: &[Expression],
        span: Span,
        scope: ScopeId,
    ) -> ValueType {
        let ExpressionKind::Name(name) = &callee.kind else {
            let _ = self.expression_type(callee, scope);
            self.error(
                "ASTER-SEM-007",
                "Aster calls must name a function directly",
                callee.span,
            );
            for argument in arguments {
                let _ = self.expression_type(argument, scope);
            }
            return ValueType::Error;
        };

        if let Some(local) = self.lookup_value(scope, name) {
            self.analysis.references.push(NameReference {
                name: name.clone(),
                span: callee.span,
                target: Some(local),
            });
            for argument in arguments {
                let _ = self.expression_type(argument, scope);
            }
            self.error(
                "ASTER-SEM-007",
                format!("value `{name}` is not callable"),
                callee.span,
            );
            return ValueType::Error;
        }

        let signature = self.functions.get(name).map(|symbol| {
            let ty = self.analysis.symbols[*symbol].ty.clone();
            (*symbol, ty)
        });
        self.analysis.references.push(NameReference {
            name: name.clone(),
            span: callee.span,
            target: signature.as_ref().map(|(symbol, _)| *symbol),
        });
        let argument_types = arguments
            .iter()
            .map(|argument| self.expression_type(argument, scope))
            .collect::<Vec<_>>();
        let Some((
            _,
            SymbolType::Function {
                parameters,
                returns,
            },
        )) = signature
        else {
            self.error(
                "ASTER-SEM-008",
                format!("unknown function `{name}`"),
                callee.span,
            );
            return ValueType::Error;
        };
        if arguments.len() != parameters.len() {
            self.error(
                "ASTER-SEM-009",
                format!(
                    "function `{name}` expects {} argument(s), received {}",
                    parameters.len(),
                    arguments.len()
                ),
                span,
            );
        }
        for (index, (expected, actual)) in parameters.iter().zip(argument_types.iter()).enumerate()
        {
            self.expect_type(
                *expected,
                *actual,
                arguments[index].span,
                &format!("argument {} to `{name}`", index + 1),
            );
        }
        returns
    }
}
