//! Generic semantic facts and workspace symbol resolution.

mod host;
mod symbols;
mod types;

pub use host::*;

use langmeld_core::{
    ConstructSpec, Diagnostic, FieldKind, LanguageSpec, MapKeyKind, SourceFile, SourceId, Span,
    ValidatedLanguageSpec,
};
use std::collections::HashMap;

use crate::{AstNode, CapturedValue, Program, Value};
use symbols::{AnalysisIndexes, SymbolKey, collect_workspace_symbols};
use types::collect_program_type_facts;

/// Host implementation that replaces generic workspace symbol resolution.
pub type WorkspaceSymbolResolverHook = Box<
    dyn for<'source> Fn(
            &ValidatedLanguageSpec,
            &[(&'source SourceFile, &'source Program)],
            &mut WorkspaceAnalysis,
        ) + Send
        + Sync,
>;

/// Runtime category observed for a generic AST value or map key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueType {
    /// An identifier token.
    Identifier,
    /// A finite numeric value.
    Number,
    /// A quoted text value.
    Text,
    /// A declared enum value.
    Enum,
    /// An unresolved or resolved reference spelling.
    Reference,
    /// A named property record.
    Properties,
    /// An ordered list.
    List,
    /// A key/value map.
    Map,
    /// A source-spanned parsed expression tree.
    Expression,
}

/// One segment in a path to a captured source value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypePathSegment {
    /// A declared construct or property field.
    Field(String),
    /// An item in a list, using its zero-based index.
    ListItem(usize),
    /// An entry in a map, using its zero-based source order.
    MapItem(usize),
    /// A key token in a map.
    MapKey(String),
    /// A value associated with a map key.
    MapValue(String),
}

/// The declared and observed type at a source value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeFact {
    /// Index into the workspace input slice.
    pub source_index: usize,
    /// Stable source identity when supplied by the caller.
    pub source_id: Option<SourceId>,
    /// Revision analyzed when supplied by the caller.
    pub source_revision: Option<u64>,
    /// Owning syntax construct.
    pub construct: String,
    /// Path from the construct to this value.
    pub path: Vec<TypePathSegment>,
    /// Type declared for this value.
    pub expected: FieldKind,
    /// Runtime type found in the parsed value.
    pub actual: ValueType,
    /// Source span of the value or map key.
    pub span: Span,
}

/// A diagnostic associated with one entry in an `analyze_workspace` input slice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceDiagnostic {
    /// Index into the input file slice passed to `analyze_workspace`.
    pub source_index: usize,
    /// Stable source identity when the workspace input supplied one.
    pub source_id: Option<SourceId>,
    /// Source revision used when the diagnostic was computed.
    pub source_revision: Option<u64>,
    /// Finding for that source file.
    pub diagnostic: Diagnostic,
}

/// A reference linked to the declaration selected by workspace name resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedReference {
    /// Index into the input file slice where the reference occurs.
    pub source_index: usize,
    /// Stable identity of the file containing the reference, when provided.
    pub source_id: Option<SourceId>,
    /// Revision of the file containing the reference, when provided.
    pub source_revision: Option<u64>,
    /// Source range of the reference spelling.
    pub reference_span: Span,
    /// Index into the input file slice containing the declaration.
    pub declaration_source_index: usize,
    /// Stable identity of the declaration file, when provided.
    pub declaration_source_id: Option<SourceId>,
    /// Revision of the declaration file, when provided.
    pub declaration_source_revision: Option<u64>,
    /// Source range of the declaration name.
    pub declaration_span: Span,
    /// Declared symbol kind.
    pub kind: String,
    /// Resolved symbol name.
    pub name: String,
}

/// Diagnostics, resolved reference edges, and generic type facts from analysis.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceAnalysis {
    /// Findings with indices into the analyzed input file slice.
    pub diagnostics: Vec<WorkspaceDiagnostic>,
    /// References whose targets were resolved to declarations.
    pub resolved_references: Vec<ResolvedReference>,
    /// Declared and observed value types for captured source fields.
    pub type_facts: Vec<TypeFact>,
    /// Host-inferred types for custom syntax and embedded expressions.
    pub host_type_facts: Vec<HostTypeFact>,
    /// Whether a configured analysis limit truncated or skipped work.
    pub resource_limited: bool,
}

/// A parsed file and its stable identity/revision for one workspace analysis.
#[derive(Clone, Copy, Debug)]
pub struct WorkspaceFile<'a> {
    /// Stable identity assigned by the source database.
    pub source_id: SourceId,
    /// Revision of this source snapshot.
    pub revision: u64,
    /// Source text and display name.
    pub source: &'a SourceFile,
    /// Parsed program for this exact source snapshot.
    pub program: &'a Program,
}

/// Work and output bounds for generic workspace analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnalysisLimits {
    /// Maximum number of source/AST pairs analyzed together.
    pub max_files: usize,
    /// Maximum AST nodes, values, and map entries inspected before analysis starts.
    pub max_work_items: usize,
    /// Maximum nested syntax and collection values traversed.
    pub max_nesting_depth: usize,
    /// Maximum returned generic and host-inferred type facts combined.
    pub max_type_facts: usize,
    /// Maximum returned resolved references.
    pub max_resolved_references: usize,
    /// Maximum returned diagnostics, including the resource-limit marker.
    pub max_diagnostics: usize,
}

impl Default for AnalysisLimits {
    fn default() -> Self {
        Self {
            max_files: 2_048,
            max_work_items: 1_000_000,
            max_nesting_depth: crate::MAX_NESTING_DEPTH,
            max_type_facts: 1_000_000,
            max_resolved_references: 500_000,
            max_diagnostics: 100_000,
        }
    }
}

/// Checks one parsed program and returns its semantic diagnostics.
#[must_use]
pub fn analyze(
    spec: &ValidatedLanguageSpec,
    program: &Program,
    source: &SourceFile,
) -> Vec<Diagnostic> {
    analyze_workspace_with_resolutions(spec, &[(source, program)])
        .diagnostics
        .into_iter()
        .map(|item| item.diagnostic)
        .collect()
}

/// Resolves declarations and references across parsed source files and collects type facts.
#[must_use]
pub fn analyze_workspace(
    spec: &ValidatedLanguageSpec,
    files: &[(&SourceFile, &Program)],
) -> Vec<WorkspaceDiagnostic> {
    analyze_workspace_with_resolutions(spec, files).diagnostics
}

/// Analyzes workspace symbols and returns reference-to-declaration links.
#[must_use]
pub fn analyze_workspace_with_resolutions(
    spec: &ValidatedLanguageSpec,
    files: &[(&SourceFile, &Program)],
) -> WorkspaceAnalysis {
    analyze_workspace_with_limits(spec, files, AnalysisLimits::default())
}

/// Analyzes a workspace with explicit work and output bounds.
#[must_use]
pub fn analyze_workspace_with_limits(
    spec: &ValidatedLanguageSpec,
    files: &[(&SourceFile, &Program)],
    limits: AnalysisLimits,
) -> WorkspaceAnalysis {
    analyze_workspace_with_host_callbacks_and_limits(spec, files, &[], &[], &[], &[], limits)
}

fn analyze_workspace_with_resolver_hook_and_limits(
    spec: &ValidatedLanguageSpec,
    files: &[(&SourceFile, &Program)],
    semantic_hooks: &[SemanticHook],
    configured_semantic_hooks: &[ConfiguredSemanticHook],
    resolver_hooks: &[WorkspaceSymbolResolverHook],
    limits: AnalysisLimits,
) -> WorkspaceAnalysis {
    let limits = effective_limits(limits);
    if let Some(reason) = workspace_analysis_limit_reason(files, limits) {
        return limited_analysis(reason);
    }

    let indexes = AnalysisIndexes::new(spec.spec());
    let mut declarations: HashMap<SymbolKey<'_>, (usize, Span)> = HashMap::new();
    let mut references = Vec::new();
    let mut diagnostics = Vec::new();
    let mut resolved_references = Vec::new();
    let mut type_facts = Vec::new();

    for (source_index, (_, program)) in files.iter().enumerate() {
        collect_program_type_facts(
            &indexes.constructs_by_id,
            &program.nodes,
            source_index,
            &mut type_facts,
            &mut diagnostics,
        );
        if resolver_hooks.is_empty() {
            collect_workspace_symbols(
                &indexes,
                &program.nodes,
                &mut Vec::new(),
                source_index,
                &mut declarations,
                &mut references,
                &mut diagnostics,
            );
        }
    }

    for reference in references {
        let declaration = reference.scopes.iter().find_map(|scope| {
            declarations
                .get(&SymbolKey {
                    kind: reference.kind,
                    scope: scope.clone(),
                    name: reference.name,
                })
                .copied()
        });
        if let Some((declaration_source_index, declaration_span)) = declaration {
            resolved_references.push(ResolvedReference {
                source_index: reference.source_index,
                source_id: None,
                source_revision: None,
                reference_span: reference.span,
                declaration_source_index,
                declaration_source_id: None,
                declaration_source_revision: None,
                declaration_span,
                kind: reference.kind.to_owned(),
                name: reference.name.to_owned(),
            });
        } else {
            diagnostics.push(WorkspaceDiagnostic {
                source_index: reference.source_index,
                source_id: None,
                source_revision: None,
                diagnostic: Diagnostic::error(
                    "LM2001",
                    format!(
                        "unknown {} `{}` in this scope",
                        reference.kind, reference.name
                    ),
                    reference.span,
                ),
            });
        }
    }

    let mut analysis = WorkspaceAnalysis {
        diagnostics,
        resolved_references,
        type_facts,
        host_type_facts: Vec::new(),
        resource_limited: false,
    };
    let callback_diagnostic_start = analysis.diagnostics.len();

    if !resolver_hooks.is_empty() {
        analysis.resolved_references.clear();
        analysis
            .diagnostics
            .retain(|item| item.diagnostic.code != "LM2001");
        for hook in resolver_hooks {
            hook(spec, files, &mut analysis);
        }
    }

    for (source_index, (source, program)) in files.iter().enumerate() {
        for hook in semantic_hooks {
            let mut hook_diagnostics = Vec::new();
            hook(program, source, &mut hook_diagnostics);
            analysis
                .diagnostics
                .extend(
                    hook_diagnostics
                        .into_iter()
                        .map(|diagnostic| WorkspaceDiagnostic {
                            source_index,
                            source_id: None,
                            source_revision: None,
                            diagnostic,
                        }),
                );
        }
        for hook in configured_semantic_hooks {
            let mut hook_diagnostics = Vec::new();
            hook(program, source, &mut hook_diagnostics);
            analysis
                .diagnostics
                .extend(
                    hook_diagnostics
                        .into_iter()
                        .map(|diagnostic| WorkspaceDiagnostic {
                            source_index,
                            source_id: None,
                            source_revision: None,
                            diagnostic,
                        }),
                );
        }
    }
    if !resolver_hooks.is_empty()
        || !semantic_hooks.is_empty()
        || !configured_semantic_hooks.is_empty()
    {
        validate_host_outputs(&mut analysis, files, callback_diagnostic_start);
    }
    enforce_analysis_output_limits(&mut analysis, limits);
    analysis
}

/// Runs semantic checks and workspace type callbacks over generic analysis results.
#[must_use]
pub fn analyze_workspace_with_type_analysis(
    spec: &ValidatedLanguageSpec,
    files: &[(&SourceFile, &Program)],
    semantic_hooks: &[SemanticHook],
    type_hooks: &[WorkspaceTypeAnalysisHook],
) -> WorkspaceAnalysis {
    analyze_workspace_with_host_callbacks(spec, files, semantic_hooks, &[], &[], type_hooks)
}

/// Runs semantic and workspace type callbacks with explicit work/output bounds.
#[must_use]
pub fn analyze_workspace_with_type_analysis_and_limits(
    spec: &ValidatedLanguageSpec,
    files: &[(&SourceFile, &Program)],
    semantic_hooks: &[SemanticHook],
    type_hooks: &[WorkspaceTypeAnalysisHook],
    limits: AnalysisLimits,
) -> WorkspaceAnalysis {
    analyze_workspace_with_host_callbacks_and_limits(
        spec,
        files,
        semantic_hooks,
        &[],
        &[],
        type_hooks,
        limits,
    )
}

/// Runs a host symbol resolver instead of generic name resolution, then type analysis.
#[must_use]
pub fn analyze_workspace_with_symbol_resolver_and_type_analysis(
    spec: &ValidatedLanguageSpec,
    files: &[(&SourceFile, &Program)],
    semantic_hooks: &[SemanticHook],
    resolver_hooks: &[WorkspaceSymbolResolverHook],
    type_hooks: &[WorkspaceTypeAnalysisHook],
) -> WorkspaceAnalysis {
    analyze_workspace_with_host_callbacks(
        spec,
        files,
        semantic_hooks,
        &[],
        resolver_hooks,
        type_hooks,
    )
}

/// Runs host resolution and type analysis with explicit work/output bounds.
#[must_use]
pub fn analyze_workspace_with_symbol_resolver_and_type_analysis_and_limits(
    spec: &ValidatedLanguageSpec,
    files: &[(&SourceFile, &Program)],
    semantic_hooks: &[SemanticHook],
    resolver_hooks: &[WorkspaceSymbolResolverHook],
    type_hooks: &[WorkspaceTypeAnalysisHook],
    limits: AnalysisLimits,
) -> WorkspaceAnalysis {
    analyze_workspace_with_host_callbacks_and_limits(
        spec,
        files,
        semantic_hooks,
        &[],
        resolver_hooks,
        type_hooks,
        limits,
    )
}

/// Runs host semantic, resolver, and type callbacks over parsed workspace files.
#[must_use]
pub fn analyze_workspace_with_host_callbacks(
    spec: &ValidatedLanguageSpec,
    files: &[(&SourceFile, &Program)],
    semantic_hooks: &[SemanticHook],
    configured_semantic_hooks: &[ConfiguredSemanticHook],
    resolver_hooks: &[WorkspaceSymbolResolverHook],
    type_hooks: &[WorkspaceTypeAnalysisHook],
) -> WorkspaceAnalysis {
    analyze_workspace_with_host_callbacks_and_limits(
        spec,
        files,
        semantic_hooks,
        configured_semantic_hooks,
        resolver_hooks,
        type_hooks,
        AnalysisLimits::default(),
    )
}

/// Runs host callbacks with explicit work and output bounds.
#[must_use]
pub fn analyze_workspace_with_host_callbacks_and_limits(
    spec: &ValidatedLanguageSpec,
    files: &[(&SourceFile, &Program)],
    semantic_hooks: &[SemanticHook],
    configured_semantic_hooks: &[ConfiguredSemanticHook],
    resolver_hooks: &[WorkspaceSymbolResolverHook],
    type_hooks: &[WorkspaceTypeAnalysisHook],
    limits: AnalysisLimits,
) -> WorkspaceAnalysis {
    let mut analysis = analyze_workspace_with_resolver_hook_and_limits(
        spec,
        files,
        semantic_hooks,
        configured_semantic_hooks,
        resolver_hooks,
        limits,
    );
    if analysis.resource_limited {
        return analysis;
    }
    let callback_diagnostic_start = analysis.diagnostics.len();
    for hook in type_hooks {
        hook(
            spec,
            files,
            &analysis.type_facts,
            &analysis.resolved_references,
            &mut analysis.host_type_facts,
            &mut analysis.diagnostics,
        );
    }
    if !type_hooks.is_empty() {
        validate_host_outputs(&mut analysis, files, callback_diagnostic_start);
    }
    enforce_analysis_output_limits(&mut analysis, effective_limits(limits));
    analysis
}

/// Analyzes source-indexed files and attaches their stable IDs and revisions.
#[must_use]
pub fn analyze_workspace_files(
    spec: &ValidatedLanguageSpec,
    files: &[WorkspaceFile<'_>],
) -> WorkspaceAnalysis {
    analyze_workspace_files_with_limits(spec, files, AnalysisLimits::default())
}

/// Analyzes source-indexed files with explicit work and output bounds.
#[must_use]
pub fn analyze_workspace_files_with_limits(
    spec: &ValidatedLanguageSpec,
    files: &[WorkspaceFile<'_>],
    limits: AnalysisLimits,
) -> WorkspaceAnalysis {
    let limits = effective_limits(limits);
    if files.len() > limits.max_files {
        let mut analysis = limited_analysis(format!(
            "workspace contains {} files; analysis limit is {}",
            files.len(),
            limits.max_files
        ));
        if let (Some(diagnostic), Some(file)) = (analysis.diagnostics.first_mut(), files.first()) {
            diagnostic.source_id = Some(file.source_id);
            diagnostic.source_revision = Some(file.revision);
        }
        return analysis;
    }
    let indexed_files = files
        .iter()
        .map(|file| (file.source, file.program))
        .collect::<Vec<_>>();
    let mut analysis = analyze_workspace_with_limits(spec, &indexed_files, limits);
    attach_source_identity(&mut analysis, files);
    analysis
}

/// Runs host callbacks on source-indexed files and attaches stable source identities.
#[must_use]
pub fn analyze_workspace_files_with_host_callbacks(
    spec: &ValidatedLanguageSpec,
    files: &[WorkspaceFile<'_>],
    semantic_hooks: &[SemanticHook],
    configured_semantic_hooks: &[ConfiguredSemanticHook],
    resolver_hooks: &[WorkspaceSymbolResolverHook],
    type_hooks: &[WorkspaceTypeAnalysisHook],
) -> WorkspaceAnalysis {
    analyze_workspace_files_with_host_callbacks_and_limits(
        spec,
        files,
        semantic_hooks,
        configured_semantic_hooks,
        resolver_hooks,
        type_hooks,
        AnalysisLimits::default(),
    )
}

/// Runs host callbacks on source-indexed files with explicit work/output bounds.
#[must_use]
pub fn analyze_workspace_files_with_host_callbacks_and_limits(
    spec: &ValidatedLanguageSpec,
    files: &[WorkspaceFile<'_>],
    semantic_hooks: &[SemanticHook],
    configured_semantic_hooks: &[ConfiguredSemanticHook],
    resolver_hooks: &[WorkspaceSymbolResolverHook],
    type_hooks: &[WorkspaceTypeAnalysisHook],
    limits: AnalysisLimits,
) -> WorkspaceAnalysis {
    let limits = effective_limits(limits);
    if files.len() > limits.max_files {
        let mut analysis = limited_analysis(format!(
            "workspace contains {} files; analysis limit is {}",
            files.len(),
            limits.max_files
        ));
        if let (Some(diagnostic), Some(file)) = (analysis.diagnostics.first_mut(), files.first()) {
            diagnostic.source_id = Some(file.source_id);
            diagnostic.source_revision = Some(file.revision);
        }
        return analysis;
    }
    let indexed_files = files
        .iter()
        .map(|file| (file.source, file.program))
        .collect::<Vec<_>>();
    let mut analysis = analyze_workspace_with_host_callbacks_and_limits(
        spec,
        &indexed_files,
        semantic_hooks,
        configured_semantic_hooks,
        resolver_hooks,
        type_hooks,
        limits,
    );
    attach_source_identity(&mut analysis, files);
    analysis
}

fn attach_source_identity(analysis: &mut WorkspaceAnalysis, files: &[WorkspaceFile<'_>]) {
    for diagnostic in &mut analysis.diagnostics {
        if let Some(file) = files.get(diagnostic.source_index) {
            diagnostic.source_id = Some(file.source_id);
            diagnostic.source_revision = Some(file.revision);
        }
    }
    for reference in &mut analysis.resolved_references {
        if let Some(file) = files.get(reference.source_index) {
            reference.source_id = Some(file.source_id);
            reference.source_revision = Some(file.revision);
        }
        if let Some(file) = files.get(reference.declaration_source_index) {
            reference.declaration_source_id = Some(file.source_id);
            reference.declaration_source_revision = Some(file.revision);
        }
    }
    for fact in &mut analysis.type_facts {
        if let Some(file) = files.get(fact.source_index) {
            fact.source_id = Some(file.source_id);
            fact.source_revision = Some(file.revision);
        }
    }
    for fact in &mut analysis.host_type_facts {
        if let Some(file) = files.get(fact.source_index) {
            fact.source_id = Some(file.source_id);
            fact.source_revision = Some(file.revision);
        }
    }
}

#[derive(Default)]
struct InputMetrics {
    work_items: usize,
    type_facts: usize,
    references: usize,
}

enum WorkItem<'a> {
    Nodes(std::slice::Iter<'a, AstNode>, usize),
    Node(&'a AstNode, usize),
    Fields(
        std::collections::btree_map::Values<'a, String, CapturedValue>,
        usize,
    ),
    Captured(&'a CapturedValue, usize),
    Value(&'a Value, usize),
    List(std::slice::Iter<'a, CapturedValue>, usize),
    Map(std::slice::Iter<'a, crate::MapEntry>, usize),
    MapEntry(&'a crate::MapEntry, usize),
}

fn effective_limits(mut limits: AnalysisLimits) -> AnalysisLimits {
    limits.max_nesting_depth = limits.max_nesting_depth.min(crate::MAX_NESTING_DEPTH);
    limits
}

fn workspace_analysis_limit_reason(
    files: &[(&SourceFile, &Program)],
    limits: AnalysisLimits,
) -> Option<String> {
    if files.len() > limits.max_files {
        return Some(format!(
            "workspace contains {} files; analysis limit is {}",
            files.len(),
            limits.max_files
        ));
    }
    let Some(metrics) = count_analysis_input(files, limits) else {
        return Some(format!(
            "workspace analysis exceeds its work or nesting limit ({} items, depth {})",
            limits.max_work_items, limits.max_nesting_depth
        ));
    };
    if metrics.type_facts > limits.max_type_facts {
        return Some(format!(
            "workspace may produce {} type facts; analysis limit is {}",
            metrics.type_facts, limits.max_type_facts
        ));
    }
    if metrics.references > limits.max_resolved_references {
        return Some(format!(
            "workspace contains {} reference values; analysis limit is {}",
            metrics.references, limits.max_resolved_references
        ));
    }
    None
}

fn count_analysis_input(
    files: &[(&SourceFile, &Program)],
    limits: AnalysisLimits,
) -> Option<InputMetrics> {
    let mut work = files
        .iter()
        .map(|(_, program)| WorkItem::Nodes(program.nodes.iter(), 0))
        .collect::<Vec<_>>();
    let mut metrics = InputMetrics::default();
    while let Some(item) = work.pop() {
        metrics.work_items = metrics.work_items.saturating_add(1);
        if metrics.work_items > limits.max_work_items {
            return None;
        }
        match item {
            WorkItem::Nodes(mut nodes, depth) => {
                if let Some(node) = nodes.next() {
                    work.push(WorkItem::Nodes(nodes, depth));
                    work.push(WorkItem::Node(node, depth.saturating_add(1)));
                }
            }
            WorkItem::Node(node, depth) => {
                if depth > limits.max_nesting_depth {
                    return None;
                }
                work.push(WorkItem::Nodes(node.children.iter(), depth));
                work.push(WorkItem::Fields(node.fields.values(), depth));
            }
            WorkItem::Fields(mut fields, depth) => {
                if let Some(value) = fields.next() {
                    work.push(WorkItem::Fields(fields, depth));
                    work.push(WorkItem::Captured(value, depth.saturating_add(1)));
                }
            }
            WorkItem::Captured(captured, depth) => {
                if depth > limits.max_nesting_depth {
                    return None;
                }
                metrics.type_facts = metrics.type_facts.saturating_add(1);
                work.push(WorkItem::Value(&captured.value, depth));
            }
            WorkItem::Value(value, depth) => match value {
                Value::Properties(fields) => {
                    work.push(WorkItem::Fields(fields.values(), depth));
                }
                Value::List(values) => work.push(WorkItem::List(values.iter(), depth)),
                Value::Map(entries) => work.push(WorkItem::Map(entries.iter(), depth)),
                Value::Reference(_) => {
                    metrics.references = metrics.references.saturating_add(1);
                }
                Value::Identifier(_)
                | Value::Number(_)
                | Value::Text(_)
                | Value::Enum(_)
                | Value::Expression(_) => {}
            },
            WorkItem::List(mut values, depth) => {
                if let Some(value) = values.next() {
                    work.push(WorkItem::List(values, depth));
                    work.push(WorkItem::Captured(value, depth.saturating_add(1)));
                }
            }
            WorkItem::Map(mut entries, depth) => {
                if let Some(entry) = entries.next() {
                    work.push(WorkItem::Map(entries, depth));
                    work.push(WorkItem::MapEntry(entry, depth.saturating_add(1)));
                }
            }
            WorkItem::MapEntry(entry, depth) => {
                if depth > limits.max_nesting_depth {
                    return None;
                }
                metrics.type_facts = metrics.type_facts.saturating_add(1);
                work.push(WorkItem::Captured(&entry.value, depth));
            }
        }
    }
    Some(metrics)
}

fn limited_analysis(reason: String) -> WorkspaceAnalysis {
    WorkspaceAnalysis {
        diagnostics: vec![WorkspaceDiagnostic {
            source_index: 0,
            source_id: None,
            source_revision: None,
            diagnostic: Diagnostic::error(
                "LM2090",
                format!("workspace analysis stopped: {reason}"),
                Span::new(0, 0),
            ),
        }],
        resolved_references: Vec::new(),
        type_facts: Vec::new(),
        host_type_facts: Vec::new(),
        resource_limited: true,
    }
}

fn valid_span(source: &SourceFile, span: Span) -> bool {
    span.start <= span.end
        && span.end <= source.text.len()
        && source.text.is_char_boundary(span.start)
        && source.text.is_char_boundary(span.end)
}

fn validate_host_outputs(
    analysis: &mut WorkspaceAnalysis,
    files: &[(&SourceFile, &Program)],
    diagnostic_start: usize,
) {
    let mut invalid_output = false;
    analysis.diagnostics = analysis
        .diagnostics
        .drain(..)
        .enumerate()
        .filter_map(|(index, item)| {
            if index < diagnostic_start {
                return Some(item);
            }
            let Some((source, _)) = files.get(item.source_index) else {
                invalid_output = true;
                return None;
            };
            let valid = item
                .diagnostic
                .span
                .is_none_or(|span| valid_span(source, span));
            invalid_output |= !valid;
            valid.then_some(item)
        })
        .collect();
    analysis.resolved_references.retain(|reference| {
        let (Some((source, _)), Some((declaration_source, _))) = (
            files.get(reference.source_index),
            files.get(reference.declaration_source_index),
        ) else {
            invalid_output = true;
            return false;
        };
        let valid = valid_span(source, reference.reference_span)
            && valid_span(declaration_source, reference.declaration_span);
        invalid_output |= !valid;
        valid
    });
    analysis.host_type_facts.retain(|fact| {
        let Some((source, _)) = files.get(fact.source_index) else {
            invalid_output = true;
            return false;
        };
        let valid = valid_span(source, fact.span);
        invalid_output |= !valid;
        valid
    });
    if invalid_output {
        analysis.diagnostics.push(WorkspaceDiagnostic {
            source_index: 0,
            source_id: None,
            source_revision: None,
            diagnostic: Diagnostic::error(
                "LM2091",
                "host analysis callback returned an invalid file index or source span",
                Span::new(0, 0),
            ),
        });
    }
}

fn enforce_analysis_output_limits(analysis: &mut WorkspaceAnalysis, limits: AnalysisLimits) {
    let mut truncated = false;
    let fact_count = analysis
        .type_facts
        .len()
        .saturating_add(analysis.host_type_facts.len());
    if fact_count > limits.max_type_facts {
        let generic_limit = analysis.type_facts.len().min(limits.max_type_facts);
        analysis.type_facts.truncate(generic_limit);
        analysis
            .host_type_facts
            .truncate(limits.max_type_facts.saturating_sub(generic_limit));
        truncated = true;
    }
    if analysis.resolved_references.len() > limits.max_resolved_references {
        analysis
            .resolved_references
            .truncate(limits.max_resolved_references);
        truncated = true;
    }
    let diagnostic_limit = limits.max_diagnostics.max(1);
    if analysis.diagnostics.len() > diagnostic_limit {
        analysis.diagnostics.truncate(diagnostic_limit);
        truncated = true;
    }
    if truncated {
        analysis.resource_limited = true;
        if analysis.diagnostics.len() >= diagnostic_limit {
            analysis.diagnostics.pop();
        }
        analysis.diagnostics.push(WorkspaceDiagnostic {
            source_index: 0,
            source_id: None,
            source_revision: None,
            diagnostic: Diagnostic::error(
                "LM2090",
                "workspace analysis output exceeded a configured limit; results were truncated",
                Span::new(0, 0),
            ),
        });
    }
}
