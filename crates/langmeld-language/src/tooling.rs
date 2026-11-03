use std::{collections::BTreeSet, fmt::Write};

use langmeld_core::{FieldKind, FieldSpec, LanguageSpec, ValidatedLanguageSpec};

use crate::{
    CLASSIFICATION_TYPES, PreparedLanguage, generate_tree_sitter, rust_codegen::generate_rust_ast,
};

/// Generated artifacts keyed by safe, relative destination paths.
pub type GeneratedArtifacts = std::collections::BTreeMap<String, String>;

/// Builds deterministic metadata, documentation, and baseline editor artifacts.
///
/// TextMate rules are lexical only. The editor configuration uses conventional
/// `#` comments and common bracket pairs because the current schema has no
/// language-specific comment or bracket settings.
pub fn generate_tooling(
    language: &ValidatedLanguageSpec,
) -> Result<GeneratedArtifacts, serde_json::Error> {
    let spec = language.spec();
    let lsp_navigation = spec
        .constructs
        .iter()
        .any(|construct| construct.symbol.is_some());
    let mut unavailable_editor_support = vec![
        "contextual_lsp_completion",
        "lsp_project_reload",
        "wasm_adapter",
        "generic_execution",
    ];
    unavailable_editor_support.push(if lsp_navigation {
        "unopened_project_file_navigation"
    } else {
        "lsp_navigation"
    });
    let (tree_sitter_support, tree_sitter_artifacts) = match generate_tree_sitter(language) {
        Ok(artifacts) => (serde_json::json!({ "available": true }), Some(artifacts)),
        Err(reason) => (
            serde_json::json!({ "available": false, "reason": reason }),
            None,
        ),
    };
    let prepared = PreparedLanguage::new(language.clone()).map_err(|error| {
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
    let mut artifacts = GeneratedArtifacts::new();
    artifacts.insert("metadata.json".to_owned(), json_file(spec)?);
    artifacts.insert(
        "highlighting.json".to_owned(),
        json_value(serde_json::json!({
            "schema_version": 1,
            "language": spec.name,
            "version": spec.version,
            "legend": CLASSIFICATION_TYPES,
            "keywords": keywords,
            "operators": operators,
            "enum_values": enum_values,
            "lexical_only": true
        }))?,
    );
    let reference = reference_document(spec);
    artifacts.insert("README.md".to_owned(), reference.clone());
    artifacts.insert("rust/ast.rs".to_owned(), generate_rust_ast(language));
    artifacts.insert(
        format!("syntaxes/{slug}.tmLanguage.json"),
        json_value(textmate_grammar(&slug, &keywords, &operators, &enum_values))?,
    );
    artifacts.insert(
        "language-configuration.json".to_owned(),
        json_value(serde_json::json!({
            "comments": { "lineComment": "#" },
            "brackets": [["{", "}"], ["[", "]"], ["(", ")"]],
            "autoClosingPairs": [["{", "}"], ["[", "]"], ["(", ")"], ["\"", "\""]]
        }))?,
    );
    artifacts.insert("agent-skill/SKILL.md".to_owned(), agent_skill(spec, &slug));
    artifacts.insert("agent-skill/references/grammar.md".to_owned(), reference);
    artifacts.insert(
        "agent-skill/references/capabilities.json".to_owned(),
        json_value(serde_json::json!({
            "schema_version": 1,
            "language": spec.name,
            "language_version": spec.version,
            "root": spec.root,
            "source_extensions": spec.source_extensions,
            "available_artifacts": [
                "typed_rust_ast", "metadata", "reference_document", "language_configuration",
                "textmate_lexical_highlighting", "agent_skill"
            ],
            "available_language_apis": ["source_formatter", "embedded_language_server"],
            "optional_artifacts": { "tree_sitter": tree_sitter_support },
            "available_cli_commands": [
                "init", "check", "validate", "format", "inspect", "constructs",
                "describe", "extension", "lock", "generate"
            ],
            "editor_support": {
                "lsp_open_document_diagnostics": true,
                "lsp_ranged_document_changes": true,
                "lsp_literal_keyword_completion": prepared.keywords().next().is_some(),
                "lsp_document_symbols": true,
                "lsp_hover": spec.constructs.iter().any(|construct| {
                    !construct.fields.is_empty() || construct.symbol.is_some()
                }),
                "lsp_semantic_tokens": true,
                "lsp_contextual_completion": false,
                "lsp_navigation": lsp_navigation,
                "lsp_navigation_open_documents_only": lsp_navigation,
                "lsp_project_reload": false
            },
            "not_yet_available": unavailable_editor_support,
            "parser_support": ["declarative_patterns", "registered_custom_construct_callbacks"],
            "editor_configuration_assumptions": {
                "line_comment": "#",
                "bracket_pairs": [["{", "}"], ["[", "]"], ["(", ")"]],
                "reason": "The language schema does not currently declare comment syntax or bracket pairs."
            },
            "highlighting": {"source": "TextMate regular expressions", "lexical_only": true},
            "requires_host_implementation_for": ["execution", "domain_lowering", "custom_type_systems"]
        }))?,
    );
    if let Some(tree_sitter_artifacts) = tree_sitter_artifacts {
        artifacts.extend(tree_sitter_artifacts);
    }

    Ok(artifacts)
}

fn json_file(spec: &LanguageSpec) -> Result<String, serde_json::Error> {
    json_value(serde_json::to_value(spec)?)
}

fn json_value(value: serde_json::Value) -> Result<String, serde_json::Error> {
    Ok(serde_json::to_string_pretty(&value)? + "\n")
}

fn agent_skill(spec: &LanguageSpec, slug: &str) -> String {
    let title = single_line(&spec.name);
    let version = single_line(&spec.version);
    let suffixes = if spec.source_extensions.is_empty() {
        "No source suffixes are declared.".to_owned()
    } else {
        format!(
            "Declared source suffixes: {}.",
            spec.source_extensions
                .iter()
                .map(|suffix| code_span(suffix))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    format!(
        "---\nname: {slug}-language\ndescription: Guidance for source written in the {slug} language.\n---\n\n# {title} language\n\nUse this guidance when creating or editing source for `{title}` version {}. {suffixes}\n\n## Workflow\n\n1. Read [the grammar reference](references/grammar.md) and [the current capabilities](references/capabilities.json).\n2. Use the declared syntax, fields, constraints, and construct relationships.\n3. Use `langmeld check` or the initialized host CLI to validate source. The standalone LSP provides diagnostics, versioned full/ranged document changes, literal-keyword suggestions, document symbols, schema-based hover, semantic tokens, and generic definition/references for declared symbols across open documents. Rust hosts can embed `langmeld_lsp::serve_configured` with registered parser and analysis callbacks; the standalone binary does not load host code.\n4. Resolve diagnostics before treating a source file as valid.\n5. Run the host application's own renderer, compiler, or interpreter to check domain behavior; Langmeld does not provide generic execution or domain lowering.\n\n## Generated artifacts\n\nThe generated TextMate grammar is lexical and the language configuration uses documented conventional comment and bracket assumptions. Neither provides parser-backed editor behavior. The generic LSP offers global literal-keyword completion, document symbols, schema-based hover, and generic definition/references across open documents; it does not load unopened project source files or infer host-specific semantics.\n",
        code_span(&version)
    )
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

fn single_line(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
}

fn code_span(value: &str) -> String {
    let max_ticks = value
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat(max_ticks + 1);
    format!("{fence}{value}{fence}")
}

fn collect_enum_values(spec: &LanguageSpec) -> BTreeSet<String> {
    let mut values = BTreeSet::new();
    for enumeration in &spec.enums {
        values.extend(enumeration.values.iter().cloned());
    }
    for record in &spec.types {
        for field in &record.fields {
            collect_kind_values(&field.kind, &mut values);
        }
    }
    for construct in &spec.constructs {
        for field in &construct.fields {
            collect_kind_values(&field.kind, &mut values);
        }
    }
    values
}

fn collect_kind_values(kind: &FieldKind, output: &mut BTreeSet<String>) {
    match kind {
        FieldKind::Enum { values } | FieldKind::NamedEnum { values, .. } => {
            output.extend(values.iter().cloned());
        }
        FieldKind::Properties { fields, .. } => {
            for field in fields {
                collect_kind_values(&field.kind, output);
            }
        }
        FieldKind::List { item } => collect_kind_values(item, output),
        FieldKind::Map { value, .. } => collect_kind_values(value, output),
        FieldKind::Identifier
        | FieldKind::Number
        | FieldKind::Text
        | FieldKind::Reference { .. } => {}
    }
}

fn collect_property_keywords(kind: &FieldKind, output: &mut BTreeSet<String>) {
    match kind {
        FieldKind::Properties { fields, .. } => {
            for field in fields {
                output.insert(field.name.clone());
                collect_property_keywords(&field.kind, output);
            }
        }
        FieldKind::List { item } => collect_property_keywords(item, output),
        FieldKind::Map { value, .. } => collect_property_keywords(value, output),
        FieldKind::Identifier
        | FieldKind::Number
        | FieldKind::Text
        | FieldKind::Enum { .. }
        | FieldKind::NamedEnum { .. }
        | FieldKind::Reference { .. } => {}
    }
}

fn collect_operators(spec: &LanguageSpec) -> BTreeSet<String> {
    let mut operators = BTreeSet::new();
    for construct in &spec.constructs {
        for pattern in &construct.syntax {
            collect_pattern_operators(pattern, &mut operators);
        }
    }
    for rule in &spec.rules {
        collect_pattern_operators(&rule.pattern, &mut operators);
    }
    operators
}

fn collect_pattern_operators(pattern: &str, output: &mut BTreeSet<String>) {
    let mut chars = pattern.chars().peekable();
    let mut token = String::new();
    let mut capture_depth = 0_usize;
    let mut quoted = false;
    while let Some(character) = chars.next() {
        if character == '\\' {
            if let Some(escaped) = chars.next()
                && capture_depth == 0
                && !quoted
                && !escaped.is_alphanumeric()
                && !escaped.is_whitespace()
            {
                token.push(escaped);
            }
            continue;
        }
        if character == '"' && capture_depth == 0 {
            flush_operator(&mut token, output);
            quoted = !quoted;
            continue;
        }
        if quoted {
            continue;
        }
        match character {
            '{' => {
                flush_operator(&mut token, output);
                capture_depth += 1;
            }
            '}' if capture_depth > 0 => {
                capture_depth -= 1;
            }
            '[' | ']' | '(' | ')' | '|' | '*' | '+' | '?' if capture_depth == 0 => {
                flush_operator(&mut token, output);
            }
            c if capture_depth > 0 => {
                let _ = c;
            }
            c if c.is_alphanumeric() || c == '_' || c.is_whitespace() => {
                flush_operator(&mut token, output)
            }
            c => token.push(c),
        }
    }
    flush_operator(&mut token, output);
}

fn flush_operator(token: &mut String, output: &mut BTreeSet<String>) {
    if !token.is_empty() {
        output.insert(std::mem::take(token));
    }
}

fn reference_document(spec: &LanguageSpec) -> String {
    let mut output = format!(
        "# {} reference\n\nVersion: {}\n\nRoot construct: {}\n\n",
        single_line(&spec.name),
        code_span(&spec.version),
        code_span(&spec.root)
    );
    if !spec.source_extensions.is_empty() {
        output.push_str("Source file suffixes: ");
        output.push_str(
            &spec
                .source_extensions
                .iter()
                .map(|value| code_span(value))
                .collect::<Vec<_>>()
                .join(", "),
        );
        output.push_str(".\n\n");
    }
    if !spec.categories.is_empty() {
        output.push_str("## Categories\n\n");
        for category in &spec.categories {
            let _ = writeln!(output, "- {}", code_span(&category.id));
        }
        output.push('\n');
    }
    if !spec.enums.is_empty() {
        output.push_str("## Reusable enums\n\n");
        for enumeration in &spec.enums {
            let values = enumeration
                .values
                .iter()
                .map(|value| code_span(value))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(output, "- {}: {values}", code_span(&enumeration.id));
        }
        output.push('\n');
    }
    if !spec.types.is_empty() {
        output.push_str("## Reusable records\n\n");
        for record in &spec.types {
            let _ = writeln!(output, "### {}\n", code_span(&record.id));
            for field in &record.fields {
                let presence = if field.required {
                    "required"
                } else {
                    "optional"
                };
                let default = field.default.as_ref().map_or_else(String::new, |value| {
                    format!(", default: {}", code_span(value))
                });
                let constraints = field_constraints_description(&field.constraints);
                let _ = writeln!(
                    output,
                    "- {}: {}, {presence}{default}{constraints}",
                    code_span(&field.name),
                    field_type_description(&field.kind)
                );
            }
        }
        output.push('\n');
    }
    if !spec.rules.is_empty() {
        output.push_str("## Reusable syntax rules\n\n");
        for rule in &spec.rules {
            let parameters = if rule.parameters.is_empty() {
                String::new()
            } else {
                format!(
                    " with parameters {}",
                    rule.parameters
                        .iter()
                        .map(|value| code_span(value))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            let _ = writeln!(
                output,
                "- {}{parameters}: {}",
                code_span(&rule.id),
                code_span(&rule.pattern)
            );
        }
        output.push('\n');
    }
    if !spec.behaviors.is_empty() {
        output.push_str("## Required host behaviors\n\n");
        for behavior in &spec.behaviors {
            let stage = match behavior.stage {
                langmeld_core::BehaviorStage::SemanticAnalysis => "semantic analysis",
                langmeld_core::BehaviorStage::SymbolResolution => "symbol resolution",
                langmeld_core::BehaviorStage::WorkspaceTypeAnalysis => "workspace type analysis",
            };
            let _ = writeln!(output, "- {} ({stage})", code_span(&behavior.id));
        }
        output.push('\n');
    }
    for construct in &spec.constructs {
        let _ = writeln!(output, "## {}\n\nSyntax:\n", code_span(&construct.id));
        if !construct.implements.is_empty() {
            let values = construct
                .implements
                .iter()
                .map(|value| code_span(value))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(output, "Implements: {values}\n");
        }
        for (index, pattern) in construct.syntax.iter().enumerate() {
            let id = construct
                .syntax_ids
                .get(index)
                .map_or_else(|| format!("variant {}", index + 1), |id| code_span(id));
            let _ = writeln!(output, "- {id}: {}", code_span(pattern));
        }
        if let Some(parser) = &construct.custom_parser {
            let _ = writeln!(
                output,
                "\nParsed by the host callback {}.",
                code_span(parser)
            );
        }
        if !construct.fields.is_empty() {
            output.push_str("\nFields:\n\n");
            for field in &construct.fields {
                let presence = if field.required {
                    "required"
                } else {
                    "optional"
                };
                let kind = field_type_description(&field.kind);
                let default = field.default.as_ref().map_or_else(String::new, |value| {
                    format!(", default: {}", code_span(value))
                });
                let constraints = field_constraints_description(&field.constraints);
                let _ = writeln!(
                    output,
                    "- {}: {kind}, {presence}{default}{constraints}",
                    code_span(&field.name)
                );
                append_nested_property_fields(&mut output, &field.name, &field.kind, "  ");
            }
        }
        if let Some(block) = &construct.block {
            let allowed = block
                .allowed
                .iter()
                .map(|value| code_span(value))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(output, "\nAllowed in the body: {allowed}.");
        }
        if let Some(symbol) = &construct.symbol {
            let scope = symbol.scope.as_ref().map_or_else(
                || "the workspace".to_owned(),
                |scope| {
                    format!(
                        "{} named by {}",
                        code_span(&scope.construct),
                        code_span(&scope.name_field)
                    )
                },
            );
            let _ = writeln!(
                output,
                "\nDeclares a {} symbol using field {} in scope {scope}.",
                code_span(&symbol.kind),
                code_span(&symbol.name_field)
            );
        }
        output.push('\n');
    }
    output
}

fn field_type_description(kind: &FieldKind) -> String {
    match kind {
        FieldKind::Identifier => "identifier".to_owned(),
        FieldKind::Number => "number".to_owned(),
        FieldKind::Text => "text".to_owned(),
        FieldKind::Enum { values } => format!(
            "enum ({})",
            values
                .iter()
                .map(|value| code_span(value))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        FieldKind::NamedEnum { name, values } => format!(
            "enum {} ({})",
            code_span(name),
            values
                .iter()
                .map(|value| code_span(value))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        FieldKind::Reference { target } => format!("reference to {}", code_span(target)),
        FieldKind::Properties { name, fields } => {
            let label = name.as_ref().map_or_else(
                || "properties".to_owned(),
                |id| format!("record {}", code_span(id)),
            );
            format!(
                "{label} with {}",
                fields
                    .iter()
                    .map(|field| code_span(&field.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        FieldKind::List { item } => format!("list of {}", field_type_description(item)),
        FieldKind::Map { key, value } => format!(
            "map keyed by {} with {} values",
            map_key_kind_name(key),
            field_type_description(value)
        ),
    }
}

fn map_key_kind_name(key: &langmeld_core::MapKeyKind) -> &'static str {
    match key {
        langmeld_core::MapKeyKind::Identifier => "identifiers",
        langmeld_core::MapKeyKind::Text => "text strings",
    }
}

fn field_constraints_description(constraints: &langmeld_core::FieldConstraints) -> String {
    let mut items = Vec::new();
    if let Some(value) = constraints.min {
        items.push(format!("minimum: {value}"));
    }
    if let Some(value) = constraints.max {
        items.push(format!("maximum: {value}"));
    }
    if let Some(value) = constraints.min_length {
        items.push(format!("minimum length: {value}"));
    }
    if let Some(value) = constraints.max_length {
        items.push(format!("maximum length: {value}"));
    }
    if let Some(value) = constraints.min_items {
        items.push(format!("minimum items: {value}"));
    }
    if let Some(value) = constraints.max_items {
        items.push(format!("maximum items: {value}"));
    }
    if items.is_empty() {
        String::new()
    } else {
        format!(", constraints: {}", items.join(", "))
    }
}

fn append_nested_property_fields(
    output: &mut String,
    parent: &str,
    kind: &FieldKind,
    indent: &str,
) {
    let Some(fields) = property_fields(kind) else {
        return;
    };
    let _ = writeln!(output, "{indent}Property fields for {}:", code_span(parent));
    for field in fields {
        let presence = if field.required {
            "required"
        } else {
            "optional"
        };
        let default = field.default.as_ref().map_or_else(String::new, |value| {
            format!(", default: {}", code_span(value))
        });
        let constraints = field_constraints_description(&field.constraints);
        let _ = writeln!(
            output,
            "{indent}- {}: {}, {presence}{default}{constraints}",
            code_span(&field.name),
            field_type_description(&field.kind)
        );
        append_nested_property_fields(output, &field.name, &field.kind, &format!("{indent}  "));
    }
}

fn property_fields(kind: &FieldKind) -> Option<&[FieldSpec]> {
    match kind {
        FieldKind::Properties { fields, .. } => Some(fields),
        FieldKind::List { item } => property_fields(item),
        FieldKind::Map { value, .. } => property_fields(value),
        _ => None,
    }
}

fn textmate_grammar(
    slug: &str,
    keywords: &BTreeSet<String>,
    operators: &BTreeSet<String>,
    enum_values: &BTreeSet<String>,
) -> serde_json::Value {
    let mut patterns = vec![
        serde_json::json!({"name": "comment.line.number-sign", "match": "#.*$"}),
        serde_json::json!({"name": "string.quoted.double", "begin": "\\\"", "end": "\\\""}),
        serde_json::json!({"name": "constant.numeric", "match": "\\b-?[0-9]+(\\.[0-9]+)?\\b"}),
    ];
    if !keywords.is_empty() {
        patterns.push(
            serde_json::json!({"name": "keyword.control.langmeld", "match": word_regex(keywords)}),
        );
    }
    if !operators.is_empty() {
        patterns.push(serde_json::json!({"name": "keyword.operator.langmeld", "match": alternative_regex(operators)}));
    }
    if !enum_values.is_empty() {
        patterns.push(
            serde_json::json!({"name": "constant.language.enum", "match": word_regex(enum_values)}),
        );
    }
    serde_json::json!({"scopeName": format!("source.{slug}"), "patterns": patterns})
}

fn word_regex(values: &BTreeSet<String>) -> String {
    format!("\\b(?:{})\\b", alternative_regex(values))
}

fn alternative_regex(values: &BTreeSet<String>) -> String {
    values
        .iter()
        .map(|value| regex_escape(value))
        .collect::<Vec<_>>()
        .join("|")
}

fn regex_escape(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars() {
        if ".+*?^$()[]{}|\\".contains(character) {
            output.push('\\');
        }
        output.push(character);
    }
    output
}
