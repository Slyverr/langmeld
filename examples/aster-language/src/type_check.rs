use std::collections::{BTreeMap, BTreeSet};

use langmeld::{
    core::{Diagnostic, SourceFile, Span, ValidatedLanguageSpec},
    expression::{Atom, Expression},
    language::{
        AstNode, HostTypeFact, Program, ResolvedReference, TypeFact, Value, WorkspaceDiagnostic,
    },
};

/// Adds Aster's numeric expression and function-call checks to generic analysis.
pub(crate) fn numeric_types(
    _language: &ValidatedLanguageSpec,
    files: &[(&SourceFile, &Program)],
    _generic_facts: &[TypeFact],
    _resolved_references: &[ResolvedReference],
    host_facts: &mut Vec<HostTypeFact>,
    diagnostics: &mut Vec<WorkspaceDiagnostic>,
) {
    let functions = files
        .iter()
        .flat_map(|(_, program)| &program.nodes)
        .flat_map(|root| root.children.iter())
        .filter(|node| node.construct == "function")
        .filter_map(|node| {
            let name = identifier(node, "name")?;
            Some((name.to_owned(), parameter_names(node).len()))
        })
        .collect::<BTreeMap<_, _>>();

    for (source_index, (_, program)) in files.iter().enumerate() {
        let mut globals = BTreeSet::new();
        for root in &program.nodes {
            for node in &root.children {
                if node.construct == "function" {
                    check_function(node, source_index, &functions, host_facts, diagnostics);
                } else {
                    check_statement(
                        node,
                        source_index,
                        &mut globals,
                        &functions,
                        false,
                        host_facts,
                        diagnostics,
                    );
                }
            }
        }
    }
}

fn check_function(
    function: &AstNode,
    source_index: usize,
    functions: &BTreeMap<String, usize>,
    host_facts: &mut Vec<HostTypeFact>,
    diagnostics: &mut Vec<WorkspaceDiagnostic>,
) {
    let mut variables = BTreeSet::new();
    let mut statement_seen = false;
    let mut return_seen = false;
    let name = identifier(function, "name").unwrap_or("<unnamed>");
    for child in &function.children {
        if child.construct == "parameter" {
            if statement_seen {
                report(
                    source_index,
                    diagnostics,
                    "ASTER1001",
                    "function parameters must appear before statements",
                    child.span,
                );
            }
            if let Some(parameter) = identifier(child, "name") {
                variables.insert(parameter.to_owned());
            }
            continue;
        }

        statement_seen = true;
        if return_seen {
            report(
                source_index,
                diagnostics,
                "ASTER1002",
                "function return must be its final statement",
                child.span,
            );
        }
        if child.construct == "return" {
            return_seen = true;
        }
        check_statement(
            child,
            source_index,
            &mut variables,
            functions,
            true,
            host_facts,
            diagnostics,
        );
    }
    if !return_seen {
        report(
            source_index,
            diagnostics,
            "ASTER1003",
            &format!("function `{name}` must return a numeric value"),
            function.span,
        );
    }
}

fn check_statement(
    node: &AstNode,
    source_index: usize,
    variables: &mut BTreeSet<String>,
    functions: &BTreeMap<String, usize>,
    in_function: bool,
    host_facts: &mut Vec<HostTypeFact>,
    diagnostics: &mut Vec<WorkspaceDiagnostic>,
) {
    match node.construct.as_str() {
        "let" => {
            if let Some(expression) = expression(node, "value") {
                check_expression(expression, source_index, variables, host_facts, diagnostics);
            }
            if let Some(name) = identifier(node, "name") {
                variables.insert(name.to_owned());
            }
        }
        "return" => {
            if !in_function {
                report(
                    source_index,
                    diagnostics,
                    "ASTER1004",
                    "return is only valid inside a function",
                    node.span,
                );
            }
            if let Some(expression) = expression(node, "value") {
                check_expression(expression, source_index, variables, host_facts, diagnostics);
            }
        }
        "call" => {
            let Some(target) = reference(node, "target") else {
                return;
            };
            if let Some(expected) = functions.get(target) {
                let actual = match node.fields.get("arguments").map(|field| &field.value) {
                    Some(Value::List(items)) => items.len(),
                    _ => 0,
                };
                if actual != *expected {
                    report(
                        source_index,
                        diagnostics,
                        "ASTER1005",
                        &format!(
                            "function `{target}` expects {expected} argument(s), received {actual}"
                        ),
                        node.fields
                            .get("arguments")
                            .map_or(node.span, |field| field.span),
                    );
                }
            }
            if let Some(name) = identifier(node, "name") {
                variables.insert(name.to_owned());
            }
        }
        "print" | "assert" => {}
        _ => {}
    }
}

fn check_expression(
    expression: &Expression,
    source_index: usize,
    variables: &BTreeSet<String>,
    host_facts: &mut Vec<HostTypeFact>,
    diagnostics: &mut Vec<WorkspaceDiagnostic>,
) {
    let mut names = Vec::new();
    expression_names(expression, &mut names);
    for (name, span) in names {
        if !variables.contains(name) {
            report(
                source_index,
                diagnostics,
                "ASTER1006",
                &format!("unknown numeric variable `{name}`"),
                span,
            );
        }
    }
    host_facts.push(HostTypeFact::new(source_index, expression.span(), "number"));
}

fn expression_names<'a>(expression: &'a Expression, names: &mut Vec<(&'a str, Span)>) {
    match expression {
        Expression::Atom(Atom::Name(name), span) => names.push((name, *span)),
        Expression::Prefix { operand, .. } => expression_names(operand, names),
        Expression::Infix { left, right, .. } => {
            expression_names(left, names);
            expression_names(right, names);
        }
        Expression::Group { expression, .. } => expression_names(expression, names),
        Expression::Call {
            callee, arguments, ..
        } => {
            expression_names(callee, names);
            for argument in arguments {
                expression_names(argument, names);
            }
        }
        Expression::Member { receiver, .. } => expression_names(receiver, names),
        Expression::Index {
            receiver, index, ..
        } => {
            expression_names(receiver, names);
            expression_names(index, names);
        }
        Expression::Atom(Atom::Number(_) | Atom::Boolean(_) | Atom::Custom { .. }, _) => {}
    }
}

fn parameter_names(function: &AstNode) -> Vec<String> {
    function
        .children
        .iter()
        .filter(|child| child.construct == "parameter")
        .filter_map(|child| identifier(child, "name").map(str::to_owned))
        .collect()
}

fn identifier<'a>(node: &'a AstNode, field: &str) -> Option<&'a str> {
    match &node.fields.get(field)?.value {
        Value::Identifier(value) => Some(value),
        _ => None,
    }
}

fn reference<'a>(node: &'a AstNode, field: &str) -> Option<&'a str> {
    match &node.fields.get(field)?.value {
        Value::Reference(value) => Some(value),
        _ => None,
    }
}

fn expression<'a>(node: &'a AstNode, field: &str) -> Option<&'a Expression> {
    match &node.fields.get(field)?.value {
        Value::Expression(value) => Some(value),
        _ => None,
    }
}

fn report(
    source_index: usize,
    diagnostics: &mut Vec<WorkspaceDiagnostic>,
    code: &'static str,
    message: &str,
    span: Span,
) {
    diagnostics.push(WorkspaceDiagnostic {
        source_index,
        source_id: None,
        source_revision: None,
        diagnostic: Diagnostic::error(code, message, span),
    });
}
