use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use langmeld_core::{Diagnostic, SourceFile, SourceLocation, Span};
use langmeld_language::{
    AnalysisDocument, LanguageAnalyzer, LanguageWorkspaceAnalyzer, ParsedDocument,
};

use crate::{Schema, SchemaParser};

/// Index of a record in a workspace analysis' symbol list.
pub type RecordId = usize;

/// A record name in the schema's global type namespace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordSymbol {
    /// Declared record name.
    pub name: String,
    /// Source range of the declaration.
    pub declaration: Span,
    /// Location of the declaration, including its source file.
    pub location: SourceLocation,
}

/// A user-defined type use and its resolved record, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeReference {
    /// Referenced type spelling.
    pub name: String,
    /// Source range of the type use.
    pub span: Span,
    /// Location of the type use, including its source file.
    pub location: SourceLocation,
    /// Index of the resolved record in [`Analysis::symbols`].
    pub target: Option<RecordId>,
}

/// Schema-specific declarations and type references.
///
/// Workspace results share these indexes across documents. The `Arc` keeps
/// per-document analysis results cheap to clone while navigation can see
/// references originating in other open files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Analysis {
    /// Unique record declarations in workspace source order.
    pub symbols: Arc<[RecordSymbol]>,
    /// User-defined field type uses in workspace source order.
    pub references: Arc<[TypeReference]>,
}

/// Resolves named record types in one schema source.
#[must_use]
pub fn analyze(source: &SourceFile, schema: &Schema) -> AnalysisDocument<Analysis> {
    analyze_sources(&[(source, schema)])
        .pop()
        .expect("one source produces one analysis")
}

fn analyze_sources(sources: &[(&SourceFile, &Schema)]) -> Vec<AnalysisDocument<Analysis>> {
    let mut symbols = Vec::new();
    let mut record_ids = HashMap::new();
    let mut references = Vec::new();
    let mut diagnostics = vec![Vec::new(); sources.len()];

    for (source_index, (source, schema)) in sources.iter().enumerate() {
        for record in &schema.records {
            if record_ids.contains_key(record.name.as_str()) {
                diagnostics[source_index].push(Diagnostic::error(
                    "SCHEMA-SEM-001",
                    format!("record `{}` is already declared", record.name),
                    source,
                    record.name_span,
                ));
                continue;
            }

            let id = symbols.len();
            symbols.push(RecordSymbol {
                name: record.name.clone(),
                declaration: record.name_span,
                location: source_location(source, record.name_span),
            });
            record_ids.insert(record.name.as_str(), id);
        }
    }

    for (source_index, (source, schema)) in sources.iter().enumerate() {
        for record in &schema.records {
            let mut fields = HashSet::new();
            for field in &record.fields {
                if !fields.insert(field.name.as_str()) {
                    diagnostics[source_index].push(Diagnostic::error(
                        "SCHEMA-SEM-002",
                        format!(
                            "field `{}` is already declared in `{}`",
                            field.name, record.name
                        ),
                        source,
                        field.name_span,
                    ));
                }

                if matches!(field.ty.name.as_str(), "int" | "bool" | "string") {
                    continue;
                }
                let target = record_ids.get(field.ty.name.as_str()).copied();
                references.push(TypeReference {
                    name: field.ty.name.clone(),
                    span: field.ty.span,
                    location: source_location(source, field.ty.span),
                    target,
                });
                if target.is_none() {
                    diagnostics[source_index].push(Diagnostic::error(
                        "SCHEMA-SEM-003",
                        format!("unknown record type `{}`", field.ty.name),
                        source,
                        field.ty.span,
                    ));
                }
            }
        }
    }

    let symbols: Arc<[RecordSymbol]> = symbols.into();
    let references: Arc<[TypeReference]> = references.into();
    sources
        .iter()
        .zip(diagnostics)
        .map(|((source, _), diagnostics)| {
            AnalysisDocument::new(
                source,
                Analysis {
                    symbols: Arc::clone(&symbols),
                    references: Arc::clone(&references),
                },
                diagnostics,
            )
        })
        .collect()
}

fn source_location(source: &SourceFile, span: Span) -> SourceLocation {
    SourceLocation {
        source_name: source.name.clone(),
        identity: source.identity(),
        span,
    }
}

/// Analyzer for schema record names and field types.
#[derive(Clone, Copy, Debug, Default)]
pub struct SchemaAnalyzer;

impl LanguageAnalyzer<SchemaParser> for SchemaAnalyzer {
    type Analysis = Analysis;

    fn analyze(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<Schema>,
    ) -> AnalysisDocument<Self::Analysis> {
        analyze(source, &parsed.tree)
    }
}

/// Resolves record types across all currently open schema documents.
#[derive(Clone, Copy, Debug, Default)]
pub struct SchemaWorkspaceAnalyzer;

impl LanguageWorkspaceAnalyzer<SchemaParser, SchemaAnalyzer> for SchemaWorkspaceAnalyzer {
    fn analyze_workspace(
        &self,
        _analyzer: &SchemaAnalyzer,
        documents: &[(&SourceFile, &ParsedDocument<Schema>)],
    ) -> Vec<AnalysisDocument<Analysis>> {
        let sources = documents
            .iter()
            .map(|(source, parsed)| (*source, &parsed.tree))
            .collect::<Vec<_>>();
        analyze_sources(&sources)
    }
}
