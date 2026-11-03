use langmeld::core::{Diagnostic, SourceFile, ValidatedLanguageSpec};
use langmeld::language::{
    analyze_workspace_with_host_callbacks, classify_parse_result_prepared,
    parse_language_definition_modules_with_limits, AnalysisLimits, CompletionBlockContext,
    CompletionFrontier, CompletionPart, DefinitionLimits, ExpectedCapture, ParseLimits,
    ParseResult, PreparedLanguage, Program, WorkspaceAnalysis,
};
use serde::Deserialize;
use serde_json::{json, Map, Value as JsonValue};
use wasm_bindgen::prelude::*;

const MAX_REQUEST_BYTES: usize = 16 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Deserialize)]
struct DefinitionBundle {
    root: DefinitionSource,
    modules: Vec<DefinitionSource>,
}

#[derive(Deserialize)]
struct DefinitionSource {
    name: String,
    text: String,
}

#[derive(Deserialize)]
struct WorkspaceSource {
    name: String,
    text: String,
}

#[derive(Clone, Copy)]
enum AnalysisMode {
    Generic,
    ProjectHost,
}

/// A bounded in-memory parser and generic editor-analysis host for JavaScript.
#[wasm_bindgen]
pub struct LanguageHost {
    spec: ValidatedLanguageSpec,
    parser: langmeld::language::ConfiguredLanguageParser,
    analysis_mode: AnalysisMode,
}

#[wasm_bindgen]
impl LanguageHost {
    /// Creates a generic host from `{ root: { name, text }, modules: [...] }` JSON.
    ///
    /// Sources are parsed in memory. The caller resolves imports and supplies every
    /// module; custom Rust parser callbacks are available through `fromProject`.
    #[wasm_bindgen(constructor)]
    pub fn new(definitions_json: &str) -> Result<LanguageHost, JsValue> {
        check_request_size(definitions_json)?;
        let definitions: DefinitionBundle = serde_json::from_str(definitions_json)
            .map_err(|error| JsValue::from_str(&format!("invalid definition bundle: {error}")))?;
        let root = SourceFile::new(definitions.root.name, definitions.root.text);
        let modules = definitions
            .modules
            .into_iter()
            .map(|module| SourceFile::new(module.name, module.text))
            .collect::<Vec<_>>();
        let spec = parse_language_definition_modules_with_limits(
            &root,
            &modules,
            DefinitionLimits::default(),
        )
        .map_err(|diagnostics| diagnostics_error(&diagnostics))?;
        Self::from_validated_spec(spec)
    }

    /// Creates a generic host from canonical `LanguageSpec` JSON.
    ///
    /// Models that declare a custom parser must use `fromProject`, where Rust
    /// callback implementations are available.
    #[wasm_bindgen(js_name = fromSpec)]
    pub fn from_spec_json(spec_json: &str) -> Result<LanguageHost, JsValue> {
        check_request_size(spec_json)?;
        let spec: langmeld::core::LanguageSpec =
            serde_json::from_str(spec_json).map_err(|error| {
                JsValue::from_str(&format!("invalid language specification: {error}"))
            })?;
        let spec = spec.validate().map_err(|errors| {
            JsValue::from_str(
                &errors
                    .iter()
                    .map(|error| format!("{}: {}", error.code, error.message))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        })?;
        Self::from_validated_spec(spec)
    }

    /// Creates a host from this generated project's compiled model and Rust callbacks.
    #[wasm_bindgen(js_name = fromProject)]
    pub fn from_project() -> Result<LanguageHost, JsValue> {
        let spec = crate::load_language().map_err(|errors| diagnostics_error(&errors))?;
        let parser = crate::parser().map_err(|errors| diagnostics_error(&errors))?;
        Ok(Self {
            spec,
            parser,
            analysis_mode: AnalysisMode::ProjectHost,
        })
    }

    /// Parses source and returns generic AST, diagnostics, classifications, and completion context.
    ///
    /// Returned spans are UTF-8 byte offsets. Opaque Rust host data is not serialized.
    pub fn parse(&self, source_text: &str) -> Result<String, JsValue> {
        let source = SourceFile::new("<browser>", source_text);
        let parsed = self
            .parser
            .parse_with_limits(&source, ParseLimits::default());
        let programs = [&parsed.program];
        let analysis = self.analyze_sources(std::slice::from_ref(&source), &programs)?;
        let mut diagnostics = parsed.diagnostics.clone();
        diagnostics.extend(
            analysis
                .diagnostics
                .iter()
                .filter(|item| item.source_index == 0)
                .map(|item| item.diagnostic.clone()),
        );
        let response = json!({
            "program": program_json(&parsed.program),
            "diagnostics": diagnostics,
            "classification_types": langmeld::language::CLASSIFICATION_TYPES,
            "classifications": classifications_json(&self.parser, &source, &parsed),
            "completion_frontiers": completion_frontiers_json(&parsed.completion_frontiers),
            "completion_blocks": completion_blocks_json(&parsed.completion_blocks),
            "resolved_references": references_json(&analysis),
            "resource_limited": parsed.resource_limited || analysis.resource_limited,
        });
        serialize_bounded(&response)
    }

    /// Parses named source values and returns cross-file generic analysis as JSON.
    ///
    /// The input is a JSON array of `{ name, text }` objects. It does not read
    /// project files or resolve paths.
    #[wasm_bindgen(js_name = analyzeWorkspace)]
    pub fn analyze_workspace_json(&self, sources_json: &str) -> Result<String, JsValue> {
        check_request_size(sources_json)?;
        let inputs: Vec<WorkspaceSource> = serde_json::from_str(sources_json).map_err(|error| {
            JsValue::from_str(&format!("invalid workspace source list: {error}"))
        })?;
        if inputs.is_empty() {
            return Err(JsValue::from_str(
                "workspace must contain at least one source",
            ));
        }
        if inputs.len() > AnalysisLimits::default().max_files {
            return Err(JsValue::from_str(
                "workspace contains more than 2,048 source files",
            ));
        }
        if inputs.iter().any(|source| source.name.trim().is_empty()) {
            return Err(JsValue::from_str(
                "workspace source names must not be empty",
            ));
        }
        let sources = inputs
            .into_iter()
            .map(|source| SourceFile::new(source.name, source.text))
            .collect::<Vec<_>>();
        let parsed = sources
            .iter()
            .map(|source| {
                self.parser
                    .parse_with_limits(source, ParseLimits::default())
            })
            .collect::<Vec<_>>();
        let programs = parsed
            .iter()
            .map(|result| &result.program)
            .collect::<Vec<_>>();
        let analysis = self.analyze_sources(&sources, &programs)?;
        let files = sources
            .iter()
            .zip(&parsed)
            .map(|(source, result)| {
                json!({
                    "name": source.name,
                    "program": program_json(&result.program),
                    "diagnostics": result.diagnostics,
                    "classifications": classifications_json(&self.parser, source, result),
                    "completion_frontiers": completion_frontiers_json(&result.completion_frontiers),
                    "completion_blocks": completion_blocks_json(&result.completion_blocks),
                    "resource_limited": result.resource_limited,
                })
            })
            .collect::<Vec<_>>();
        let response = json!({
            "files": files,
            "classification_types": langmeld::language::CLASSIFICATION_TYPES,
            "analysis": {
                "diagnostics": analysis.diagnostics.iter().map(workspace_diagnostic_json).collect::<Vec<_>>(),
                "resolved_references": references_json(&analysis),
                "resource_limited": analysis.resource_limited,
            },
        });
        serialize_bounded(&response)
    }
}

impl LanguageHost {
    fn from_validated_spec(spec: ValidatedLanguageSpec) -> Result<Self, JsValue> {
        let prepared = PreparedLanguage::new(spec.clone())
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        let parser = prepared
            .configure()
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        Ok(Self {
            spec,
            parser,
            analysis_mode: AnalysisMode::Generic,
        })
    }

    fn analyze_sources(
        &self,
        sources: &[SourceFile],
        programs: &[&Program],
    ) -> Result<WorkspaceAnalysis, JsValue> {
        let files = sources
            .iter()
            .zip(programs.iter().copied())
            .collect::<Vec<_>>();
        match self.analysis_mode {
            AnalysisMode::Generic => Ok(analyze_workspace_with_host_callbacks(
                &self.spec,
                &files,
                &[],
                &[],
                &[],
                &[],
            )),
            AnalysisMode::ProjectHost => crate::analyze_sources(&self.spec, sources, programs)
                .map_err(|error| JsValue::from_str(&error)),
        }
    }
}

fn check_request_size(value: &str) -> Result<(), JsValue> {
    if value.len() > MAX_REQUEST_BYTES {
        return Err(JsValue::from_str(
            "serialized input exceeds the 16 MiB limit",
        ));
    }
    Ok(())
}

fn serialize_bounded(value: &JsonValue) -> Result<String, JsValue> {
    let response = serde_json::to_string(value)
        .map_err(|error| JsValue::from_str(&format!("cannot serialize response: {error}")))?;
    if response.len() > MAX_RESPONSE_BYTES {
        return Err(JsValue::from_str(
            "serialized response exceeds the 64 MiB limit",
        ));
    }
    Ok(response)
}

fn diagnostics_error(diagnostics: &[Diagnostic]) -> JsValue {
    serde_json::to_string(diagnostics)
        .map(|message| JsValue::from_str(&message))
        .unwrap_or_else(|_| JsValue::from_str("language model could not be loaded"))
}

fn program_json(program: &Program) -> JsonValue {
    json!({"nodes": program.nodes.iter().map(node_json).collect::<Vec<_>>()})
}

fn node_json(node: &langmeld::language::AstNode) -> JsonValue {
    let fields = node
        .fields
        .iter()
        .map(|(name, captured)| (name.clone(), captured_json(captured)))
        .collect::<Map<_, _>>();
    json!({
        "construct": node.construct,
        "syntax_variant": node.syntax_variant,
        "fields": fields,
        "children": node.children.iter().map(node_json).collect::<Vec<_>>(),
        "span": node.span,
    })
}

fn captured_json(captured: &langmeld::language::CapturedValue) -> JsonValue {
    json!({"value": value_json(&captured.value), "span": captured.span})
}

fn value_json(value: &langmeld::language::Value) -> JsonValue {
    use langmeld::language::Value;
    match value {
        Value::Identifier(value)
        | Value::Text(value)
        | Value::Enum(value)
        | Value::Reference(value) => json!(value),
        Value::Number(value) => json!(value),
        Value::Properties(fields) => {
            let fields = fields
                .iter()
                .map(|(name, value)| (name.clone(), captured_json(value)))
                .collect::<Map<_, _>>();
            JsonValue::Object(fields)
        }
        Value::List(items) => json!(items.iter().map(captured_json).collect::<Vec<_>>()),
        Value::Map(entries) => json!(entries.iter().map(map_entry_json).collect::<Vec<_>>()),
    }
}

fn map_entry_json(entry: &langmeld::language::MapEntry) -> JsonValue {
    let key_type = match entry.key_type {
        langmeld::language::ValueType::Identifier => "identifier",
        langmeld::language::ValueType::Text => "text",
        _ => "other",
    };
    json!({
        "key": entry.key,
        "key_type": key_type,
        "key_span": entry.key_span,
        "value": captured_json(&entry.value),
    })
}

fn classifications_json(
    parser: &langmeld::language::ConfiguredLanguageParser,
    source: &SourceFile,
    parsed: &ParseResult,
) -> JsonValue {
    let classifications = classify_parse_result_prepared(parser.language(), source, parsed);
    json!(classifications
        .iter()
        .map(|item| json!({"kind": item.kind, "span": item.span}))
        .collect::<Vec<_>>())
}

fn completion_frontiers_json(frontiers: &[CompletionFrontier]) -> JsonValue {
    json!(frontiers
        .iter()
        .map(completion_frontier_json)
        .collect::<Vec<_>>())
}

fn completion_frontier_json(frontier: &CompletionFrontier) -> JsonValue {
    let captures = frontier
        .captures
        .iter()
        .map(expected_capture_json)
        .collect::<Vec<_>>();
    let continuation = frontier
        .continuation
        .as_ref()
        .map(|parts| parts.iter().map(completion_part_json).collect::<Vec<_>>());
    json!({
        "span": frontier.span,
        "ambiguous": frontier.ambiguous,
        "captures": captures,
        "continuation": continuation,
    })
}

fn expected_capture_json(capture: &ExpectedCapture) -> JsonValue {
    json!({
        "construct": capture.construct,
        "syntax_variant": capture.syntax_variant,
        "field": capture.field,
        "kind": capture.kind,
    })
}

fn completion_part_json(part: &CompletionPart) -> JsonValue {
    match part {
        CompletionPart::Literal(text) => json!({"kind": "literal", "text": text}),
        CompletionPart::Capture(field) => json!({"kind": "capture", "field": field}),
    }
}

fn completion_blocks_json(blocks: &[CompletionBlockContext]) -> JsonValue {
    json!(blocks
        .iter()
        .map(|block| json!({
            "parent": block.parent,
            "allowed": block.allowed,
            "span": block.span,
        }))
        .collect::<Vec<_>>())
}

fn workspace_diagnostic_json(item: &langmeld::language::WorkspaceDiagnostic) -> JsonValue {
    json!({"source_index": item.source_index, "diagnostic": item.diagnostic})
}

fn references_json(analysis: &WorkspaceAnalysis) -> JsonValue {
    json!(analysis
        .resolved_references
        .iter()
        .map(|item| json!({
            "source_index": item.source_index,
            "reference_span": item.reference_span,
            "declaration_source_index": item.declaration_source_index,
            "declaration_span": item.declaration_span,
            "kind": item.kind,
            "name": item.name,
        }))
        .collect::<Vec<_>>())
}
