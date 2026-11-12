use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_expression::{
    Associativity, Expression, ExpressionGrammar, ExpressionKind, ExpressionParser, OperatorSpec,
};
use langmeld_language::{CapturedIdentifier, ParseResult, ParsedConstruct, TypedConstruct};

use crate::generated::constructs::{Function as FunctionSyntax, Invoke as InvokeSyntax};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Program {
    pub functions: Vec<Function>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub name_span: Span,
    pub parameters: Vec<Parameter>,
    pub return_type: String,
    pub calls: Vec<CallSite>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub name_span: Span,
    pub value_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSite {
    pub name: String,
    pub name_span: Span,
    pub argument_count: usize,
}

pub fn lower(source: &SourceFile, parsed: &ParseResult) -> (Program, Vec<Diagnostic>) {
    let grammar = ExpressionGrammar::new([
        OperatorSpec::infix("+", 1, Associativity::Left),
        OperatorSpec::infix("*", 2, Associativity::Left),
    ])
    .expect("the example expression grammar uses distinct valid operators");
    let expression_parser = ExpressionParser::new(&grammar);
    let mut program = Program::default();
    let mut diagnostics = Vec::new();

    for root in parsed.constructs() {
        for construct in &root.children {
            if construct.name != "function" {
                continue;
            }

            if let Some(function) = lower_function(
                source,
                parsed,
                construct,
                &expression_parser,
                &mut diagnostics,
            ) {
                program.functions.push(function);
            }
        }
    }

    (program, diagnostics)
}

fn lower_function(
    source: &SourceFile,
    parsed: &ParseResult,
    construct: &ParsedConstruct,
    expression_parser: &ExpressionParser<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Function> {
    let syntax = match FunctionSyntax::from_construct(source, parsed, construct) {
        Ok(syntax) => syntax,
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            return None;
        }
    };
    let name = required_identifier(syntax.name(), diagnostics)?;
    let return_type = required_identifier(syntax.return_type(), diagnostics)?;
    let parameters = lower_parameters(&syntax, diagnostics);
    let mut calls = Vec::new();

    for child in &construct.children {
        if child.name != "invoke" {
            continue;
        }

        let syntax = match InvokeSyntax::from_construct(source, parsed, child) {
            Ok(syntax) => syntax,
            Err(diagnostic) => {
                diagnostics.push(diagnostic);
                continue;
            }
        };
        let call = match syntax {
            InvokeSyntax::KeywordCall(syntax) => syntax.call(),
            InvokeSyntax::CallStatement(syntax) => syntax.call(),
        };
        let call = match call {
            Ok(call) => call,
            Err(diagnostic) => {
                diagnostics.push(diagnostic);
                continue;
            }
        };
        let parsed_expression = expression_parser.parse(source, call.span);
        diagnostics.extend(parsed_expression.diagnostics().iter().cloned());

        if !parsed_expression.is_valid() {
            continue;
        }

        if let Some(expression) = parsed_expression.expression() {
            collect_calls(expression, &mut calls);
        }
    }

    Some(Function {
        name: name.text.to_owned(),
        name_span: name.span,
        parameters,
        return_type: return_type.text.to_owned(),
        calls,
        span: construct.span,
    })
}

fn lower_parameters(
    syntax: &FunctionSyntax<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Parameter> {
    let parameters = match syntax.parameters() {
        Ok(Some(parameters)) => parameters,
        Ok(None) => return Vec::new(),
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            return Vec::new();
        }
    };
    let mut lowered = Vec::new();

    if let Some(parameter) = lower_parameter(
        parameters.first_parameter(),
        parameters.first_parameter_type(),
        diagnostics,
    ) {
        lowered.push(parameter);
    }

    match parameters.additional_parameters() {
        Ok(repeated) => {
            for parameter in repeated {
                if let Some(parameter) = lower_parameter(
                    parameter.parameter(),
                    parameter.parameter_type(),
                    diagnostics,
                ) {
                    lowered.push(parameter);
                }
            }
        }
        Err(diagnostic) => diagnostics.push(diagnostic),
    }

    lowered
}

fn lower_parameter(
    name: Result<CapturedIdentifier<'_>, Diagnostic>,
    value_type: Result<CapturedIdentifier<'_>, Diagnostic>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Parameter> {
    let name = required_identifier(name, diagnostics)?;
    let value_type = required_identifier(value_type, diagnostics)?;

    Some(Parameter {
        name: name.text.to_owned(),
        name_span: name.span,
        value_type: value_type.text.to_owned(),
    })
}

fn required_identifier<'source>(
    capture: Result<CapturedIdentifier<'source>, Diagnostic>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<CapturedIdentifier<'source>> {
    match capture {
        Ok(capture) => Some(capture),
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            None
        }
    }
}

fn collect_calls(expression: &Expression, calls: &mut Vec<CallSite>) {
    match &expression.kind {
        ExpressionKind::Call { callee, arguments } => {
            if let ExpressionKind::Name(name) = &callee.kind {
                calls.push(CallSite {
                    name: name.clone(),
                    name_span: callee.span,
                    argument_count: arguments.len(),
                });
            } else {
                collect_calls(callee, calls);
            }

            for argument in arguments {
                collect_calls(argument, calls);
            }
        }
        ExpressionKind::Group(inner) | ExpressionKind::Prefix { operand: inner, .. } => {
            collect_calls(inner, calls);
        }
        ExpressionKind::Infix { left, right, .. } => {
            collect_calls(left, calls);
            collect_calls(right, calls);
        }
        ExpressionKind::Member { target, .. } => collect_calls(target, calls),
        ExpressionKind::Index { target, index } => {
            collect_calls(target, calls);
            collect_calls(index, calls);
        }
        ExpressionKind::Integer(_) | ExpressionKind::Name(_) | ExpressionKind::String(_) => {}
    }
}
