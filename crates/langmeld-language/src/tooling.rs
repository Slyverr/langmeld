use std::collections::BTreeSet;

use langmeld_core::{LanguageSpec, LexicalSpec, ValidatedLanguageSpec};

use crate::tooling_capabilities::capability_report;
use crate::tooling_highlighting::{
    collect_enum_values, collect_operators, collect_property_keywords, textmate_grammar,
};
use crate::tooling_reference::{agent_skill, reference_document};

use crate::{
    CLASSIFICATION_TYPES, Capability, EditorConfigurationAssumptions, PatternCompileError,
    PreparedLanguage, SEMANTIC_TOKEN_MODIFIERS, SEMANTIC_TOKEN_TYPES, ToolingCapabilities,
    generate_tree_sitter, rust_codegen::generate_rust_ast,
    semantic_token_modifier_for_classification, semantic_token_type_for_classification,
    textmate_scope_for_classification, tree_sitter_capture_for_classification,
};

/// Generated artifacts keyed by safe, relative destination paths.
pub type GeneratedArtifacts = std::collections::BTreeMap<String, String>;

/// Builds deterministic metadata, documentation, and baseline editor artifacts.
///
/// TextMate rules are lexical only. The editor configuration follows the
/// language's lexical comment policy and derives bracket pairs from declared
/// blocks and structured field types.
pub fn generate_tooling(
    language: &ValidatedLanguageSpec,
) -> Result<GeneratedArtifacts, serde_json::Error> {
    let spec = language.spec();
    let default_lexical = LexicalSpec::default();
    let lexical = spec.lexical.as_ref().unwrap_or(&default_lexical);
    let (tree_sitter_capability, tree_sitter_artifacts) = tree_sitter_output(language);
    let prepared = prepare_language(language).map_err(|error| {
        serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("cannot compile validated syntax for editor tooling: {error}"),
        ))
    })?;
    let mut keywords = prepared
        .keywords()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let operators = collect_operators(spec);
    let enum_values = collect_enum_values(spec);
    for record in &spec.types {
        for field in &record.fields {
            collect_property_keywords(&field.kind, &mut keywords);
        }
    }
    for construct in &spec.constructs {
        for field in &construct.fields {
            collect_property_keywords(&field.kind, &mut keywords);
        }
    }
    let slug = safe_slug(&spec.name);
    let capabilities = capability_report(language, &prepared, tree_sitter_capability);
    let classification_mappings = CLASSIFICATION_TYPES
        .iter()
        .map(|kind| {
            serde_json::json!({
                "kind": kind,
                "semantic_token_type": semantic_token_type_for_classification(kind),
                "semantic_token_modifier": semantic_token_modifier_for_classification(kind),
                "textmate_scope": textmate_scope_for_classification(kind),
                "tree_sitter_capture": tree_sitter_capture_for_classification(kind)
            })
        })
        .collect::<Vec<_>>();
    let mut artifacts = GeneratedArtifacts::new();
    artifacts.insert("metadata.json".to_owned(), json_file(spec)?);
    artifacts.insert(
        "highlighting.json".to_owned(),
        json_value(serde_json::json!({
            "schema_version": 2,
            "language": spec.name,
            "version": spec.version,
            "legend": CLASSIFICATION_TYPES,
            "semantic_token_types": SEMANTIC_TOKEN_TYPES,
            "semantic_token_modifiers": SEMANTIC_TOKEN_MODIFIERS,
            "classification_mappings": classification_mappings,
            "keywords": keywords,
            "operators": operators,
            "enum_values": enum_values,
            "lexical_only": true
        }))?,
    );
    let reference = reference_document(spec, &capabilities);
    artifacts.insert("README.md".to_owned(), reference.clone());
    artifacts.insert("rust/ast.rs".to_owned(), generate_rust_ast(language));
    artifacts.insert(
        format!("syntaxes/{slug}.tmLanguage.json"),
        json_value(textmate_grammar(
            &slug,
            &keywords,
            &operators,
            &enum_values,
            lexical,
        ))?,
    );
    artifacts.insert(
        "language-configuration.json".to_owned(),
        json_value(language_configuration(
            &capabilities.editor_configuration_assumptions,
        ))?,
    );
    artifacts.insert(
        "agent-skill/SKILL.md".to_owned(),
        agent_skill(spec, &slug, &capabilities),
    );
    artifacts.insert("agent-skill/references/grammar.md".to_owned(), reference);
    artifacts.insert(
        "agent-skill/references/capabilities.json".to_owned(),
        json_value(serde_json::to_value(capabilities)?)?,
    );
    if let Some(tree_sitter_artifacts) = tree_sitter_artifacts {
        artifacts.extend(tree_sitter_artifacts);
    }

    Ok(artifacts)
}

/// Describes the generated artifacts and generic editor features available for
/// a validated language.
///
/// This is the same report serialized to
/// `agent-skill/references/capabilities.json` by [`generate_tooling`].
///
/// # Errors
/// Returns an error if the validated syntax cannot be compiled by the generic
/// parser.
pub fn generate_tooling_capabilities(
    language: &ValidatedLanguageSpec,
) -> Result<ToolingCapabilities, PatternCompileError> {
    let prepared = prepare_language(language)?;
    let (tree_sitter, _) = tree_sitter_output(language);
    Ok(capability_report(language, &prepared, tree_sitter))
}

fn prepare_language(
    language: &ValidatedLanguageSpec,
) -> Result<PreparedLanguage, PatternCompileError> {
    PreparedLanguage::new(language.clone())
}

fn tree_sitter_output(
    language: &ValidatedLanguageSpec,
) -> (Capability, Option<GeneratedArtifacts>) {
    match generate_tree_sitter(language) {
        Ok(artifacts) => (
            Capability::partial(
                "The generated grammar models declared syntax but does not reproduce semantic validation, such as required-field and duplicate-key checks; Langmeld remains authoritative.",
            ),
            Some(artifacts),
        ),
        Err(reason) => (Capability::unavailable(reason), None),
    }
}

fn json_file(spec: &LanguageSpec) -> Result<String, serde_json::Error> {
    json_value(serde_json::to_value(spec)?)
}

fn json_value(value: serde_json::Value) -> Result<String, serde_json::Error> {
    Ok(serde_json::to_string_pretty(&value)? + "\n")
}

fn language_configuration(assumptions: &EditorConfigurationAssumptions) -> serde_json::Value {
    let mut auto_closing_pairs = assumptions.bracket_pairs.clone();
    auto_closing_pairs.insert(("\"".to_owned(), "\"".to_owned()));
    let mut comments = serde_json::Map::new();
    if let Some(line_comment) = &assumptions.line_comment {
        comments.insert("lineComment".to_owned(), serde_json::json!(line_comment));
    }
    if let Some(block_comment) = &assumptions.block_comment {
        comments.insert("blockComment".to_owned(), serde_json::json!(block_comment));
    }
    serde_json::json!({
        "comments": comments,
        "brackets": assumptions.bracket_pairs,
        "autoClosingPairs": auto_closing_pairs
    })
}

fn safe_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut pending_separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if pending_separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(character.to_ascii_lowercase());
            pending_separator = false;
        } else {
            pending_separator = true;
        }
    }
    if slug.is_empty() {
        "language".to_owned()
    } else {
        slug
    }
}
