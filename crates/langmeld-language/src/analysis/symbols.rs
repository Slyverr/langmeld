use super::*;
use std::collections::HashMap;

pub(super) type ScopeIdentity<'a> = Vec<(&'a str, &'a str)>;

pub(super) struct AnalysisIndexes<'a> {
    pub(super) constructs_by_id: HashMap<&'a str, &'a ConstructSpec>,
    pub(super) symbol_scopes_by_kind: HashMap<&'a str, Option<&'a langmeld_core::ScopeSpec>>,
    pub(super) symbol_policies_by_kind: HashMap<&'a str, &'a langmeld_core::SymbolPolicySpec>,
}

impl<'a> AnalysisIndexes<'a> {
    pub(super) fn new(spec: &'a LanguageSpec) -> Self {
        let constructs_by_id = spec
            .constructs
            .iter()
            .map(|construct| (construct.id.as_str(), construct))
            .collect();
        let mut symbol_scopes_by_kind = HashMap::new();
        for construct in &spec.constructs {
            if let Some(symbol) = &construct.symbol {
                symbol_scopes_by_kind
                    .entry(symbol.kind.as_str())
                    .or_insert(symbol.scope.as_ref());
            }
        }
        let symbol_policies_by_kind = spec
            .symbol_policies
            .iter()
            .map(|policy| (policy.kind.as_str(), policy))
            .collect();
        Self {
            constructs_by_id,
            symbol_scopes_by_kind,
            symbol_policies_by_kind,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct SymbolKey<'a> {
    pub(super) kind: &'a str,
    pub(super) scope: Option<ScopeIdentity<'a>>,
    pub(super) name: &'a str,
}

pub(super) struct PendingReference<'a> {
    pub(super) source_index: usize,
    pub(super) kind: &'a str,
    pub(super) scopes: Vec<Option<ScopeIdentity<'a>>>,
    pub(super) name: &'a str,
    pub(super) span: Span,
}

pub(super) struct SymbolDeclaration<'a> {
    pub(super) source_index: usize,
    pub(super) span: Span,
    pub(super) scope_path: Option<ScopeIdentity<'a>>,
}

pub(super) fn collect_workspace_symbols<'a>(
    indexes: &AnalysisIndexes<'a>,
    nodes: &'a [AstNode],
    ancestors: &mut Vec<&'a AstNode>,
    source_index: usize,
    declarations: &mut HashMap<SymbolKey<'a>, Vec<SymbolDeclaration<'a>>>,
    references: &mut Vec<PendingReference<'a>>,
    diagnostics: &mut Vec<WorkspaceDiagnostic>,
) {
    for node in nodes {
        let Some(construct) = indexes.constructs_by_id.get(node.construct.as_str()) else {
            continue;
        };
        ancestors.push(node);
        let declaration = construct.symbol.as_ref().and_then(|symbol| {
            let value = node.fields.get(&symbol.name_field)?;
            let Value::Identifier(name) = &value.value else {
                return None;
            };
            Some((symbol, value, name.as_str()))
        });
        if let Some((symbol, value, name)) = declaration {
            if let Some(scope) = scope_key(symbol.scope.as_ref(), ancestors) {
                let key = SymbolKey {
                    kind: symbol.kind.as_str(),
                    scope,
                    name,
                };
                let policy = indexes.symbol_policies_by_kind.get(symbol.kind.as_str());
                let existing = declarations.entry(key).or_default();
                if !existing.is_empty()
                    && policy.is_none_or(|policy| {
                        policy.duplicates == langmeld_core::DuplicatePolicy::Error
                    })
                {
                    diagnostics.push(WorkspaceDiagnostic {
                        source_index,
                        source_id: None,
                        source_revision: None,
                        diagnostic: Diagnostic::error(
                            "LM2002",
                            format!(
                                "duplicate {} declaration `{name}` in this scope",
                                symbol.kind
                            ),
                            value.span,
                        ),
                    });
                } else {
                    let scope_path = policy
                        .is_some_and(|policy| {
                            policy.shadowing == langmeld_core::ShadowingPolicy::Disallow
                        })
                        .then(|| scope_path(symbol.scope.as_ref(), ancestors))
                        .flatten();
                    existing.push(SymbolDeclaration {
                        source_index,
                        span: value.span,
                        scope_path,
                    });
                }
            } else {
                diagnostics.push(WorkspaceDiagnostic {
                    source_index,
                    source_id: None,
                    source_revision: None,
                    diagnostic: Diagnostic::error(
                        "LM2003",
                        format!(
                            "{} declaration `{name}` is outside its declared scope",
                            symbol.kind
                        ),
                        value.span,
                    ),
                });
            }
        }
        for field in &construct.fields {
            if let Some(value) = node.fields.get(&field.name) {
                collect_field_references(
                    indexes,
                    &field.kind,
                    value,
                    ancestors,
                    source_index,
                    references,
                    diagnostics,
                );
            }
        }
        collect_workspace_symbols(
            indexes,
            &node.children,
            ancestors,
            source_index,
            declarations,
            references,
            diagnostics,
        );
        ancestors.pop();
    }
}

fn collect_field_references<'a>(
    indexes: &AnalysisIndexes<'a>,
    kind: &'a FieldKind,
    captured: &'a CapturedValue,
    ancestors: &[&'a AstNode],
    source_index: usize,
    references: &mut Vec<PendingReference<'a>>,
    diagnostics: &mut Vec<WorkspaceDiagnostic>,
) {
    match (kind, &captured.value) {
        (FieldKind::Reference { target }, Value::Reference(name)) => {
            let target_scope = indexes.symbol_scopes_by_kind.get(target.as_str()).copied();
            if let Some(scopes) = target_scope.and_then(|scope| scope_keys(scope, ancestors)) {
                references.push(PendingReference {
                    source_index,
                    kind: target,
                    scopes,
                    name,
                    span: captured.span,
                });
            } else {
                diagnostics.push(WorkspaceDiagnostic {
                    source_index,
                    source_id: None,
                    source_revision: None,
                    diagnostic: Diagnostic::error(
                        "LM2004",
                        format!("reference to {target} `{name}` is outside its declared scope"),
                        captured.span,
                    ),
                });
            }
        }
        (FieldKind::Properties { fields, .. }, Value::Properties(values)) => {
            for field in fields {
                if let Some(value) = values.get(&field.name) {
                    collect_field_references(
                        indexes,
                        &field.kind,
                        value,
                        ancestors,
                        source_index,
                        references,
                        diagnostics,
                    );
                }
            }
        }
        (FieldKind::List { item }, Value::List(values)) => {
            for value in values {
                collect_field_references(
                    indexes,
                    item,
                    value,
                    ancestors,
                    source_index,
                    references,
                    diagnostics,
                );
            }
        }
        (FieldKind::Map { value, .. }, Value::Map(values)) => {
            for entry in values {
                collect_field_references(
                    indexes,
                    value,
                    &entry.value,
                    ancestors,
                    source_index,
                    references,
                    diagnostics,
                );
            }
        }
        _ => {}
    }
}

fn scope_key<'a>(
    scope: Option<&'a langmeld_core::ScopeSpec>,
    ancestors: &[&'a AstNode],
) -> Option<Option<ScopeIdentity<'a>>> {
    let Some(scope) = scope else {
        return Some(None);
    };
    let path = scope_path(Some(scope), ancestors)?;
    Some(Some(if scope.lookup == langmeld_core::ScopeLookup::Exact {
        vec![*path.last().expect("matching scope exists")]
    } else {
        path
    }))
}

fn scope_path<'a>(
    scope: Option<&'a langmeld_core::ScopeSpec>,
    ancestors: &[&'a AstNode],
) -> Option<ScopeIdentity<'a>> {
    let scope = scope?;
    let matching: Vec<_> = ancestors
        .iter()
        .filter(|ancestor| ancestor.construct == scope.construct)
        .filter_map(|ancestor| {
            ancestor
                .fields
                .get(&scope.name_field)
                .and_then(|field| match &field.value {
                    Value::Identifier(name) => Some(name.as_str()),
                    _ => None,
                })
                .map(|name| (scope.construct.as_str(), name))
        })
        .collect();
    (!matching.is_empty()).then_some(matching)
}

fn scope_keys<'a>(
    scope: Option<&'a langmeld_core::ScopeSpec>,
    ancestors: &[&'a AstNode],
) -> Option<Vec<Option<ScopeIdentity<'a>>>> {
    let Some(scope) = scope else {
        return Some(vec![None]);
    };
    let matching: Vec<_> = ancestors
        .iter()
        .filter(|ancestor| ancestor.construct == scope.construct)
        .filter_map(|ancestor| {
            ancestor
                .fields
                .get(&scope.name_field)
                .and_then(|field| match &field.value {
                    Value::Identifier(name) => Some(name.as_str()),
                    _ => None,
                })
                .map(|name| (scope.construct.as_str(), name))
        })
        .collect();
    if matching.is_empty() {
        None
    } else if scope.lookup == langmeld_core::ScopeLookup::Exact {
        Some(vec![Some(vec![*matching.last().expect("scope exists")])])
    } else {
        Some(
            (1..=matching.len())
                .rev()
                .map(|length| Some(matching[..length].to_vec()))
                .collect(),
        )
    }
}
