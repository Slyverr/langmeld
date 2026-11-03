use std::collections::{BTreeMap, HashMap, HashSet};

use langmeld::{
    core::{Diagnostic, SourceFile, Span},
    language::{AstNode, CapturedValue, CustomConstructMatch, CustomConstructParser, Value},
};
use serde_json::Value as JsonValue;

use crate::frontend::PythonFrontend;

pub(super) fn parser_callbacks() -> [CustomConstructParser; 1] {
    let frontend = std::sync::Arc::new(std::sync::Mutex::new(PythonFrontend::new()));
    let callback_frontend = std::sync::Arc::clone(&frontend);
    [CustomConstructParser::new(
        "python_module",
        move |source, start| {
            let mut frontend = callback_frontend.lock().map_err(|_| {
                Diagnostic::spec_error("PS1000", "CPython frontend lock was poisoned")
            })?;
            parse_module(&mut frontend, source, start)
        },
    )]
}

pub(super) fn parse_module(
    frontend: &mut PythonFrontend,
    source: &SourceFile,
    start: usize,
) -> Result<Option<CustomConstructMatch>, Diagnostic> {
    let ast = frontend.parse(source)?;
    let functions = ast
        .get("functions")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| Diagnostic::spec_error("PS1002", "Python AST has no function list"))?;
    let imported_targets = ast
        .get("imports")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(|import| {
            let module = import.get("module")?.as_str()?;
            let scope = python_import_scope_name(source, module)?;
            Some((import, scope))
        })
        .flat_map(|(import, scope)| {
            import
                .get("names")
                .and_then(JsonValue::as_array)
                .into_iter()
                .flatten()
                .filter_map(move |name| {
                    Some((
                        name.get("local_name")?.as_str()?.to_owned(),
                        (
                            name.get("imported_name")?.as_str()?.to_owned(),
                            scope.clone(),
                        ),
                    ))
                })
        })
        .collect::<HashMap<_, _>>();
    let mut children = ast
        .get("imports")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(|import| import_node(source, import))
        .chain(
            functions
                .iter()
                .filter_map(|function| function_node(source, function, &imported_targets)),
        )
        .collect::<Vec<_>>();
    children.sort_by_key(|node| node.span.start);
    let diagnostics = ast
        .get("error")
        .filter(|error| !error.is_null())
        .map(|error| python_syntax_diagnostic(source, error))
        .into_iter()
        .collect();
    Ok(Some(CustomConstructMatch {
        span: Span::new(start, source.text.len()),
        fields: BTreeMap::from([(
            "name".to_owned(),
            captured(
                Value::Identifier(python_file_scope_name(&source.name)),
                Span::new(start, start),
            ),
        )]),
        children: Some(children),
        syntax_variant: "python_module".to_owned(),
        diagnostics,
        host_data: None,
    }))
}

fn import_node(source: &SourceFile, import: &JsonValue) -> Option<AstNode> {
    let module = import.get("module")?.as_str()?;
    let span = read_span(source, import.get("span")?)?;
    let module_span = read_span(source, import.get("module_span")?)?;
    let names = import.get("names")?.as_array()?;
    let names = names
        .iter()
        .map(|name| {
            let imported_name = name.get("imported_name")?.as_str()?.to_owned();
            let local_name = name.get("local_name")?.as_str()?.to_owned();
            let item_span = read_span(source, name.get("span")?)?;
            let imported_name_span = read_span(source, name.get("name_span")?)?;
            let local_name_span = read_span(source, name.get("local_name_span")?)?;
            Some(CapturedValue::new(
                Value::Properties(BTreeMap::from([
                    (
                        "imported_name".to_owned(),
                        captured(Value::Identifier(imported_name), imported_name_span),
                    ),
                    (
                        "local_name".to_owned(),
                        captured(Value::Identifier(local_name), local_name_span),
                    ),
                ])),
                item_span,
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    let names_span = Span::new(names.first()?.span.start, names.last()?.span.end);
    Some(AstNode {
        construct: "python_import".to_owned(),
        syntax_variant: "python_relative_import".to_owned(),
        fields: BTreeMap::from([
            (
                "module".to_owned(),
                captured(Value::Text(module.to_owned()), module_span),
            ),
            ("names".to_owned(), captured(Value::List(names), names_span)),
        ]),
        children: Vec::new(),
        span,
        host_data: None,
    })
}

fn function_node(
    source: &SourceFile,
    function: &JsonValue,
    imported_targets: &HashMap<String, (String, String)>,
) -> Option<AstNode> {
    let name = function.get("name")?.as_str()?.to_owned();
    let span = read_span(source, function.get("span")?)?;
    let name_span = read_span(source, function.get("name_span")?)?;
    let kind = function.get("kind")?.as_str()?;
    let mut fields = BTreeMap::from([
        (
            "name".to_owned(),
            captured(Value::Identifier(name), name_span),
        ),
        (
            "kind".to_owned(),
            captured(Value::Enum(kind.to_owned()), span),
        ),
    ]);
    if let Some(return_type) = function.get("return_type").and_then(JsonValue::as_str) {
        let return_span = read_span(source, function.get("return_type_span")?)?;
        fields.insert(
            "return_type".to_owned(),
            captured(Value::Text(return_type.to_owned()), return_span),
        );
    }
    let parameters = function
        .get("parameters")?
        .as_array()?
        .iter()
        .map(|parameter| {
            let name = parameter.get("name")?.as_str()?.to_owned();
            let name_span = read_span(source, parameter.get("name_span")?)?;
            let item_span = read_span(source, parameter.get("span")?)?;
            let mut values = BTreeMap::from([(
                "name".to_owned(),
                captured(Value::Identifier(name), name_span),
            )]);
            if let Some(annotation) = parameter.get("annotation").and_then(JsonValue::as_str) {
                let annotation_span = read_span(source, parameter.get("annotation_span")?)?;
                values.insert(
                    "annotation".to_owned(),
                    captured(Value::Text(annotation.to_owned()), annotation_span),
                );
            }
            Some(CapturedValue::new(Value::Properties(values), item_span))
        })
        .collect::<Option<Vec<_>>>()?;
    if !parameters.is_empty() {
        let parameters_span =
            Span::new(parameters.first()?.span.start, parameters.last()?.span.end);
        fields.insert(
            "parameters".to_owned(),
            captured(Value::List(parameters), parameters_span),
        );
    }

    let local_names = function
        .get("local_names")
        .and_then(JsonValue::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(JsonValue::as_str)
                .map(ToOwned::to_owned)
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();
    let calls = function
        .get("calls")?
        .as_array()?
        .iter()
        .filter_map(|call| {
            let target = call.get("target")?.as_str()?;
            let local_target = call.get("local_target")?.as_str()?;
            if !local_names.is_empty() && !local_names.contains(target) {
                return None;
            }
            let target_span = read_span(source, call.get("target_span")?)?;
            let call_span = read_span(source, call.get("span")?)?;
            let imported_target = imported_targets.get(local_target);
            let target_name = imported_target.map_or(target, |(name, _)| name).to_owned();
            let call_node = AstNode {
                construct: "call".to_owned(),
                syntax_variant: "python_call".to_owned(),
                fields: BTreeMap::from([
                    (
                        "target".to_owned(),
                        captured(Value::Reference(target_name), target_span),
                    ),
                    (
                        "local_target".to_owned(),
                        captured(Value::Identifier(local_target.to_owned()), target_span),
                    ),
                ]),
                children: Vec::new(),
                span: call_span,
                host_data: None,
            };
            Some(if let Some((_, module_scope)) = imported_target {
                AstNode {
                    construct: "module".to_owned(),
                    syntax_variant: "python_import_scope".to_owned(),
                    fields: BTreeMap::from([(
                        "name".to_owned(),
                        captured(Value::Identifier(module_scope.clone()), target_span),
                    )]),
                    children: vec![call_node],
                    span: call_span,
                    host_data: None,
                }
            } else {
                call_node
            })
        })
        .collect();
    Some(AstNode {
        construct: "function".to_owned(),
        syntax_variant: if kind == "async" {
            "python_async_function"
        } else {
            "python_function"
        }
        .to_owned(),
        fields,
        children: calls,
        span,
        host_data: None,
    })
}

fn python_syntax_diagnostic(source: &SourceFile, error: &JsonValue) -> Diagnostic {
    let message = error
        .get("message")
        .and_then(JsonValue::as_str)
        .unwrap_or("invalid Python syntax");
    match error.get("span").and_then(|span| read_span(source, span)) {
        Some(span) => Diagnostic::error("PS1001", message, span),
        None => Diagnostic::spec_error("PS1001", message),
    }
}

fn read_span(source: &SourceFile, span: &JsonValue) -> Option<Span> {
    let coordinates = span.as_array()?;
    let start = usize::try_from(coordinates.first()?.as_u64()?).ok()?;
    let end = usize::try_from(coordinates.get(1)?.as_u64()?).ok()?;
    (start <= end
        && end <= source.text.len()
        && source.text.is_char_boundary(start)
        && source.text.is_char_boundary(end))
    .then_some(Span::new(start, end))
}

fn python_file_scope_name(source_name: &str) -> String {
    crate::project::python_source_path(source_name)
        .as_deref()
        .map(crate::project::normalize_path)
        .map(crate::project::encode_python_module_path)
        .unwrap_or_else(|| crate::project::encode_python_module_path(source_name.into()))
}

fn python_import_scope_name(source: &SourceFile, module: &str) -> Option<String> {
    let importer = crate::project::python_source_path(&source.name)?;
    let parent = importer.parent()?;
    Some(crate::project::encode_python_module_path(
        crate::project::normalize_path(&parent.join(format!("{module}.py"))),
    ))
}

fn captured(value: Value, span: Span) -> CapturedValue {
    CapturedValue { value, span }
}
