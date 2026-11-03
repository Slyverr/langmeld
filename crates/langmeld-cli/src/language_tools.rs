use std::path::{Path, PathBuf};

use langmeld_core::ValidatedLanguageSpec;
use langmeld_language::{
    PreparedLanguage, generate_tree_sitter, language_definition_source_paths, load_language,
    load_project, project_definition_source_paths,
};
use serde_json::{Value, json};

use crate::cli::InspectionArgs;

pub(crate) fn inspect(arguments: InspectionArgs) -> Result<(), (u8, String)> {
    let (language, sources, base) = if let Some(path) = arguments.language {
        let language = load_language(&path).map_err(|error| (2, error))?;
        let sources = language_definition_source_paths(&path, Default::default())
            .map_err(|error| (2, error))?;
        let base = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        (language, sources, base)
    } else {
        let path = arguments
            .project
            .as_deref()
            .expect("Clap requires either a project or language input");
        let project = load_project(path).map_err(|error| (2, error))?;
        let sources =
            project_definition_source_paths(path, &project).map_err(|error| (2, error))?;
        let base = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        (project.language, sources, base)
    };
    let source_labels = labels(&sources, &base)?;
    let output = inspection(&language, source_labels);
    print_json(&output, "language inspection")
}

pub(crate) fn constructs(language_path: &Path) -> Result<(), (u8, String)> {
    let language = load_language(language_path).map_err(|error| (2, error))?;
    let values: Vec<_> = language
        .spec()
        .constructs
        .iter()
        .map(|construct| {
            json!({
                "id": construct.id,
                "syntax": construct.syntax,
                "field_count": construct.fields.len(),
                "children": construct.block.as_ref().map(|block| &block.allowed),
            })
        })
        .collect();
    print_json(&Value::Array(values), "construct list")
}

pub(crate) fn describe(language_path: &Path, id: &str) -> Result<(), (u8, String)> {
    let language = load_language(language_path).map_err(|error| (2, error))?;
    let construct = language
        .construct(id)
        .ok_or_else(|| (2, format!("unknown construct '{id}'")))?;
    let value = serde_json::to_value(construct).map_err(|error| {
        (
            2,
            format!("cannot serialize construct description: {error}"),
        )
    })?;
    print_json(&value, "construct description")
}

fn inspection(language: &ValidatedLanguageSpec, definition_sources: Vec<String>) -> Value {
    let spec = language.spec();
    let lsp_literal_keyword_completion = PreparedLanguage::new(language.clone())
        .map(|prepared| prepared.keywords().next().is_some())
        .unwrap_or(false);
    let lsp_navigation = spec
        .constructs
        .iter()
        .any(|construct| construct.symbol.is_some());
    let tree_sitter = match generate_tree_sitter(language) {
        Ok(_) => json!({ "supported": true }),
        Err(reason) => json!({ "supported": false, "reason": reason }),
    };
    json!({
        "schema_version": spec.schema_version,
        "name": spec.name,
        "version": spec.version,
        "root": spec.root,
        "definition_sources": definition_sources,
        "construct_count": spec.constructs.len(),
        "construct_ids": spec.constructs.iter().map(|construct| &construct.id).collect::<Vec<_>>(),
        "tree_sitter": tree_sitter,
        "current_capabilities": {
            "language_tool_commands": ["check", "format", "inspect", "constructs", "describe"],
            "lsp_open_document_diagnostics": true,
            "lsp_ranged_document_changes": true,
            "lsp_literal_keyword_completion": lsp_literal_keyword_completion,
            "lsp_document_symbols": true,
            "lsp_contextual_completion": false,
            "lsp_navigation": lsp_navigation,
            "lsp_navigation_open_documents_only": lsp_navigation,
            "wasm_implemented": false,
        },
        "host_implementation_required_for": [
            "language-specific execution",
            "custom parser callbacks",
            "custom type systems",
            "backend compilation",
        ],
    })
}

fn labels(paths: &[PathBuf], base: &Path) -> Result<Vec<String>, (u8, String)> {
    let root = std::fs::canonicalize(base).map_err(|error| {
        (
            2,
            format!(
                "cannot resolve definition root '{}': {error}",
                base.display()
            ),
        )
    })?;
    Ok(paths
        .iter()
        .map(|path| {
            path.strip_prefix(&root)
                .unwrap_or(path)
                .display()
                .to_string()
        })
        .collect())
}

fn print_json(value: &Value, role: &str) -> Result<(), (u8, String)> {
    let output = serde_json::to_string_pretty(value)
        .map_err(|error| (2, format!("cannot serialize {role}: {error}")))?;
    println!("{output}");
    Ok(())
}
