use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

use langmeld::{
    core::{Diagnostic, SourceFile, Span},
    language::{AstNode, Program, Value, WorkspaceDiagnostic, WorkspaceTypeAnalysisHook},
};

pub(super) fn analyze_project_imports(
    files: &[(&SourceFile, &Program)],
    diagnostics: &mut Vec<WorkspaceDiagnostic>,
) {
    let imported_call_spans = imported_call_spans(files);
    diagnostics.retain(|entry| {
        entry.diagnostic.code != "LM2001"
            || entry.diagnostic.span.is_none_or(|span| {
                !imported_call_spans.contains(&(entry.source_index, span.start, span.end))
            })
    });
    let sources = files.iter().map(|(source, _)| *source).collect::<Vec<_>>();
    for (source_index, (source, program)) in files.iter().enumerate() {
        let Some(module_node) = program.nodes.first() else {
            continue;
        };
        for import in module_node
            .children
            .iter()
            .filter(|node| node.construct == "python_import")
        {
            let Some(module) = text_field(import, "module") else {
                continue;
            };
            let module_span = import.fields.get("module").map(|field| field.span);
            let Some(target_index) = resolve_python_module(source, module, &sources) else {
                diagnostics.push(workspace_diagnostic(
                    source_index,
                    Diagnostic::error(
                        "PS1004",
                        format!(
                            "cannot resolve relative Python module `.{module}` in this project"
                        ),
                        module_span.unwrap_or(Span::new(0, 0)),
                    ),
                ));
                continue;
            };
            let Some(Value::List(names)) = import.fields.get("names").map(|field| &field.value)
            else {
                continue;
            };
            for name in names
                .iter()
                .filter_map(|name| properties_identifier(name, "imported_name"))
            {
                let exists = files[target_index]
                    .1
                    .nodes
                    .first()
                    .is_some_and(|target_module| {
                        target_module.children.iter().any(|function| {
                            function.construct == "function"
                                && identifier_field(function, "name") == Some(name)
                        })
                    });
                if !exists {
                    let span = names_span(import, name).or(module_span).unwrap_or_default();
                    diagnostics.push(workspace_diagnostic(
                        source_index,
                        Diagnostic::error(
                            "PS1005",
                            format!("`{name}` is not a top-level function in relative module `.{module}`"),
                            span,
                        ),
                    ));
                }
            }
        }
    }
}

pub(super) fn project_import_analysis_hook() -> WorkspaceTypeAnalysisHook {
    Box::new(|_, files, _, _, _, diagnostics| {
        analyze_project_imports(files, diagnostics);
    })
}

fn imported_call_spans(files: &[(&SourceFile, &Program)]) -> HashSet<(usize, usize, usize)> {
    let mut spans = HashSet::new();
    for (source_index, (_, program)) in files.iter().enumerate() {
        let Some(module) = program.nodes.first() else {
            continue;
        };
        let imported_names = module
            .children
            .iter()
            .filter(|node| node.construct == "python_import")
            .filter_map(|node| node.fields.get("names"))
            .filter_map(|field| match &field.value {
                Value::List(names) => Some(names),
                _ => None,
            })
            .flatten()
            .filter_map(|name| properties_identifier(name, "local_name"))
            .collect::<HashSet<_>>();
        for function in module
            .children
            .iter()
            .filter(|node| node.construct == "function")
        {
            for child in &function.children {
                let calls = if child.construct == "module" {
                    child.children.iter().collect::<Vec<_>>()
                } else {
                    vec![child]
                };
                for call in calls {
                    if identifier_field(call, "local_target")
                        .is_some_and(|name| imported_names.contains(name))
                        && let Some(target) = call.fields.get("target")
                    {
                        spans.insert((source_index, target.span.start, target.span.end));
                    }
                }
            }
        }
    }
    spans
}

fn names_span(import: &AstNode, imported_name: &str) -> Option<Span> {
    let Value::List(names) = &import.fields.get("names")?.value else {
        return None;
    };
    names.iter().find_map(|name| {
        (properties_identifier(name, "imported_name") == Some(imported_name))
            .then(|| match &name.value {
                Value::Properties(fields) => fields.get("imported_name").map(|field| field.span),
                _ => None,
            })
            .flatten()
    })
}

fn identifier_field<'a>(node: &'a AstNode, name: &str) -> Option<&'a str> {
    match &node.fields.get(name)?.value {
        Value::Identifier(value) => Some(value),
        _ => None,
    }
}

fn text_field<'a>(node: &'a AstNode, name: &str) -> Option<&'a str> {
    match &node.fields.get(name)?.value {
        Value::Text(value) => Some(value),
        _ => None,
    }
}

fn properties_identifier<'a>(
    value: &'a langmeld::language::CapturedValue,
    name: &str,
) -> Option<&'a str> {
    let Value::Properties(fields) = &value.value else {
        return None;
    };
    match &fields.get(name)?.value {
        Value::Identifier(value) => Some(value),
        _ => None,
    }
}

fn resolve_python_module(
    source: &SourceFile,
    module: &str,
    sources: &[&SourceFile],
) -> Option<usize> {
    if module.contains('.') {
        return None;
    }
    let importer = python_source_path(&source.name)?;
    let parent = importer.parent()?;
    let requested = normalize_path(&parent.join(format!("{module}.py")));
    sources.iter().position(|candidate| {
        python_source_path(&candidate.name).is_some_and(|path| normalize_path(&path) == requested)
    })
}

fn workspace_diagnostic(source_index: usize, diagnostic: Diagnostic) -> WorkspaceDiagnostic {
    WorkspaceDiagnostic {
        source_index,
        source_id: None,
        source_revision: None,
        diagnostic,
    }
}

pub(super) fn python_source_path(name: &str) -> Option<PathBuf> {
    let path = name.strip_prefix("file://").unwrap_or(name);
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    Some(PathBuf::from(String::from_utf8(decoded).ok()?))
}

pub(super) fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

pub(super) fn encode_python_module_path(path: PathBuf) -> String {
    use std::fmt::Write as _;

    let path = path.to_string_lossy();
    let mut scope = String::with_capacity(1 + path.len() * 2);
    scope.push('m');
    for byte in path.bytes() {
        write!(scope, "{byte:02x}").expect("writing to String cannot fail");
    }
    scope
}
