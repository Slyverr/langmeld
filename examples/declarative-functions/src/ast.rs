use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_expression::{
    Associativity, Expression, ExpressionGrammar, ExpressionKind, ExpressionParser, OperatorSpec,
};
use langmeld_language::{CapturedValue, ParseResult, ParsedConstruct};

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

            if let Some(function) =
                lower_function(source, construct, &expression_parser, &mut diagnostics)
            {
                program.functions.push(function);
            }
        }
    }

    (program, diagnostics)
}

fn lower_function(
    source: &SourceFile,
    construct: &ParsedConstruct,
    expression_parser: &ExpressionParser<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Function> {
    let name = identifier_field(source, construct, "name", diagnostics)?;
    let return_type = identifier_field(source, construct, "return_type", diagnostics)?;
    let parameters = lower_parameters(source, construct, diagnostics);

    let mut calls = Vec::new();
    for child in &construct.children {
        if child.name != "invoke" {
            continue;
        }

        let Some(field) = child.fields.iter().find(|field| field.name == "call") else {
            diagnostics.push(Diagnostic::error(
                "FUNCTION-001",
                "invoke statement has no captured call expression",
                source,
                child.span,
            ));
            continue;
        };
        let parsed_expression = expression_parser.parse(source, field.span);
        diagnostics.extend(parsed_expression.diagnostics().iter().cloned());

        if !parsed_expression.is_valid() {
            continue;
        }

        if let Some(expression) = parsed_expression.expression() {
            collect_calls(expression, &mut calls);
        }
    }

    Some(Function {
        name,
        name_span: find_field_span(construct, "name").unwrap_or(construct.span),
        parameters,
        return_type,
        calls,
        span: construct.span,
    })
}

fn lower_parameters(
    source: &SourceFile,
    construct: &ParsedConstruct,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Parameter> {
    let Some(optional_group) = construct
        .capture_groups
        .iter()
        .find(|group| group.name.as_deref() == Some("parameters"))
    else {
        return Vec::new();
    };
    let Some(parameter_list) = optional_group.occurrences.first() else {
        return Vec::new();
    };
    let Some(fields) = parameter_list.captures(construct) else {
        report_invalid_parameter_group(source, parameter_list.span, diagnostics);
        return Vec::new();
    };

    let mut parameters = Vec::new();
    if let Some(parameter) =
        lower_parameter_pair(source, fields.get(..2), parameter_list.span, diagnostics)
    {
        parameters.push(parameter);
    }

    for repeated_group in parameter_list
        .groups
        .iter()
        .filter(|group| group.name.as_deref() == Some("additional_parameters"))
    {
        for occurrence in &repeated_group.occurrences {
            let Some(fields) = occurrence.captures(construct) else {
                report_invalid_parameter_group(source, occurrence.span, diagnostics);
                continue;
            };

            if let Some(parameter) =
                lower_parameter_pair(source, Some(fields), occurrence.span, diagnostics)
            {
                parameters.push(parameter);
            }
        }
    }

    parameters
}

fn lower_parameter_pair(
    source: &SourceFile,
    fields: Option<&[langmeld_language::CapturedField]>,
    group_span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Parameter> {
    let Some([name_field, type_field]) = fields else {
        report_invalid_parameter_group(source, group_span, diagnostics);
        return None;
    };
    let name = identifier_value(source, &name_field.name, name_field, diagnostics)?;
    let value_type = identifier_value(source, &type_field.name, type_field, diagnostics)?;

    Some(Parameter {
        name,
        name_span: name_field.span,
        value_type,
    })
}

fn report_invalid_parameter_group(
    source: &SourceFile,
    span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) {
    diagnostics.push(Diagnostic::error(
        "FUNCTION-007",
        "function parameter capture group does not contain a name and type",
        source,
        span,
    ));
}

fn identifier_field(
    source: &SourceFile,
    construct: &ParsedConstruct,
    name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    let Some(field) = construct.fields.iter().find(|field| field.name == name) else {
        diagnostics.push(Diagnostic::error(
            "FUNCTION-002",
            format!("function declaration is missing its `{name}` field"),
            source,
            construct.span,
        ));
        return None;
    };

    identifier_value(source, name, field, diagnostics)
}

fn identifier_value(
    source: &SourceFile,
    name: &str,
    field: &langmeld_language::CapturedField,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    match &field.value {
        CapturedValue::Identifier(value) => Some(value.clone()),
        _ => {
            diagnostics.push(Diagnostic::error(
                "FUNCTION-003",
                format!("captured field `{name}` is not an identifier"),
                source,
                field.span,
            ));
            None
        }
    }
}

fn find_field_span(construct: &ParsedConstruct, name: &str) -> Option<Span> {
    construct
        .fields
        .iter()
        .find(|field| field.name == name)
        .map(|field| field.span)
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
