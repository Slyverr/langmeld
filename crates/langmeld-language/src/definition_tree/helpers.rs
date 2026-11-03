use crate::{AstNode, CapturedValue, Value};
use langmeld_core::Diagnostic;

pub(super) fn child<'a>(
    node: &'a AstNode,
    construct: &str,
) -> Result<&'a AstNode, Vec<Diagnostic>> {
    optional_child(node, construct)?
        .ok_or_else(|| vec![error(node, format!("missing `{construct}` declaration"))])
}

pub(super) fn optional_child<'a>(
    node: &'a AstNode,
    construct: &str,
) -> Result<Option<&'a AstNode>, Vec<Diagnostic>> {
    let mut matches = node
        .children
        .iter()
        .filter(|child| child.construct == construct);
    let found = matches.next();
    if matches.next().is_some() {
        return Err(vec![error(
            node,
            format!("duplicate `{construct}` declaration"),
        )]);
    }
    Ok(found)
}

pub(super) fn identifier(node: &AstNode, field: &str) -> Result<String, Vec<Diagnostic>> {
    match &captured(node, field)?.value {
        Value::Identifier(value) => Ok(value.clone()),
        _ => Err(vec![error(
            node,
            format!("field `{field}` must be an identifier"),
        )]),
    }
}

pub(super) fn identifiers(node: &AstNode, field: &str) -> Result<Vec<String>, Vec<Diagnostic>> {
    let Value::List(values) = &captured(node, field)?.value else {
        return Err(vec![error(node, format!("field `{field}` must be a list"))]);
    };
    values
        .iter()
        .map(|value| match &value.value {
            Value::Identifier(value) => Ok(value.clone()),
            _ => Err(vec![error(
                node,
                format!("field `{field}` must contain identifiers"),
            )]),
        })
        .collect()
}

pub(super) fn texts(node: &AstNode, field: &str) -> Result<Vec<String>, Vec<Diagnostic>> {
    let Value::List(values) = &captured(node, field)?.value else {
        return Err(vec![error(node, format!("field `{field}` must be a list"))]);
    };
    values
        .iter()
        .map(|value| match &value.value {
            Value::Text(value) => Ok(value.clone()),
            _ => Err(vec![error(
                node,
                format!("field `{field}` must contain text values"),
            )]),
        })
        .collect()
}

pub(super) fn optional_identifier<'a>(
    node: &'a AstNode,
    field: &str,
) -> Result<Option<&'a str>, Vec<Diagnostic>> {
    match node.fields.get(field).map(|value| &value.value) {
        Some(Value::Identifier(value)) => Ok(Some(value)),
        Some(_) => Err(vec![error(
            node,
            format!("field `{field}` must be an identifier"),
        )]),
        None => Ok(None),
    }
}

pub(super) fn optional_identifier_or_enum<'a>(
    node: &'a AstNode,
    field: &str,
) -> Result<Option<&'a str>, Vec<Diagnostic>> {
    match node.fields.get(field).map(|value| &value.value) {
        Some(Value::Identifier(value) | Value::Enum(value)) => Ok(Some(value)),
        Some(_) => Err(vec![error(
            node,
            format!("field `{field}` must be an identifier or enum value"),
        )]),
        None => Ok(None),
    }
}

pub(crate) fn text(node: &AstNode, field: &str) -> Result<String, Vec<Diagnostic>> {
    match &captured(node, field)?.value {
        Value::Text(value) => Ok(value.clone()),
        _ => Err(vec![error(node, format!("field `{field}` must be text"))]),
    }
}

pub(super) fn captured<'a>(
    node: &'a AstNode,
    field: &str,
) -> Result<&'a CapturedValue, Vec<Diagnostic>> {
    node.fields
        .get(field)
        .ok_or_else(|| vec![error(node, format!("missing field `{field}`"))])
}

pub(crate) fn error(node: &AstNode, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("LMBOOT001", message, node.span)
}
