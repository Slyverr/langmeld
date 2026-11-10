use std::collections::{HashMap, HashSet};

use langmeld_core::{Diagnostic, SourceFile, Span};
use langmeld_language::{AnalysisDocument, LanguageAnalyzer, ParsedDocument};

use crate::{Schema, SchemaParser};

/// Index of a record in an [`Analysis`]'s symbol list.
pub type RecordId = usize;

/// A record name in the schema's global type namespace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordSymbol {
    /// Declared record name.
    pub name: String,
    /// Source range of the declaration.
    pub declaration: Span,
}

/// A user-defined type use and its resolved record, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeReference {
    /// Referenced type name.
    pub name: String,
    /// Source range of the type use.
    pub span: Span,
    /// Index of the resolved record in [`Analysis::symbols`].
    pub target: Option<RecordId>,
}

/// Schema-specific declarations and type references.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Analysis {
    /// Unique record declarations in source order.
    pub symbols: Vec<RecordSymbol>,
    /// User-defined field type uses in source order.
    pub references: Vec<TypeReference>,
}

/// Resolves named record types after collecting the schema's global declarations.
#[must_use]
pub fn analyze(source: &SourceFile, schema: &Schema) -> AnalysisDocument<Analysis> {
    let mut analysis = Analysis::default();
    let mut diagnostics = Vec::new();
    let mut records = HashMap::new();

    for record in &schema.records {
        if records.contains_key(record.name.as_str()) {
            diagnostics.push(Diagnostic::error(
                "SCHEMA-SEM-001",
                format!("record `{}` is already declared", record.name),
                source,
                record.name_span,
            ));
            continue;
        }
        let id = analysis.symbols.len();
        analysis.symbols.push(RecordSymbol {
            name: record.name.clone(),
            declaration: record.name_span,
        });
        records.insert(record.name.as_str(), id);
    }

    for record in &schema.records {
        let mut fields = HashSet::new();
        for field in &record.fields {
            if !fields.insert(field.name.as_str()) {
                diagnostics.push(Diagnostic::error(
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
            let target = records.get(field.ty.name.as_str()).copied();
            analysis.references.push(TypeReference {
                name: field.ty.name.clone(),
                span: field.ty.span,
                target,
            });
            if target.is_none() {
                diagnostics.push(Diagnostic::error(
                    "SCHEMA-SEM-003",
                    format!("unknown record type `{}`", field.ty.name),
                    source,
                    field.ty.span,
                ));
            }
        }
    }

    AnalysisDocument::new(source, analysis, diagnostics)
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
