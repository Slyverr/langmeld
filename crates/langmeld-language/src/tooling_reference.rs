use std::{collections::BTreeMap, fmt::Write};

use langmeld_core::{FieldKind, FieldSpec, LanguageSpec};

use crate::{Capability, CapabilityStatus, EditorConfigurationAssumptions, ToolingCapabilities};

pub(crate) fn agent_skill(
    spec: &LanguageSpec,
    slug: &str,
    capabilities: &ToolingCapabilities,
) -> String {
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
    let editor_configuration =
        editor_configuration_description(&capabilities.editor_configuration_assumptions);
    format!(
        "---\nname: {slug}-language\ndescription: Guidance for source written in the {slug} language.\n---\n\n# {title} language\n\nUse this guidance when creating or editing source for `{title}` version {}. {suffixes}\n\n## Workflow\n\n1. Read [the grammar reference](references/grammar.md) and [the current capabilities](references/capabilities.json).\n2. Use the declared syntax, fields, constraints, and construct relationships.\n3. Use `langmeld check` or the initialized host CLI to validate source.\n4. Resolve diagnostics before treating a source file as valid.\n5. Run the host application's own renderer, compiler, or interpreter to check domain behavior; Langmeld does not provide generic execution or domain lowering.\n\n## Generated artifacts\n\nThe grammar reference includes generated artifact and language-feature support tables based on this language's validated capability report. Check them before choosing an editor integration or relying on host-specific behavior. TextMate is lexical; it does not provide parser-backed or semantic highlighting. {editor_configuration}\n",
        code_span(&version)
    )
}

fn editor_configuration_description(assumptions: &EditorConfigurationAssumptions) -> String {
    let mut comment_delimiters = Vec::new();
    if let Some(line_comment) = &assumptions.line_comment {
        comment_delimiters.push(format!("line comments use {}", code_span(line_comment)));
    }
    if let Some([open, close]) = &assumptions.block_comment {
        comment_delimiters.push(format!(
            "block comments use {} and {}",
            code_span(open),
            code_span(close)
        ));
    }
    let comment_delimiters = if comment_delimiters.is_empty() {
        "no comment delimiters are configured".to_owned()
    } else {
        comment_delimiters.join("; ")
    };
    format!(
        "The generated language configuration uses {comment_delimiters}. {}",
        assumptions.reason
    )
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

pub(crate) fn reference_document(
    spec: &LanguageSpec,
    capabilities: &ToolingCapabilities,
) -> String {
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
            let separator_policy = match block.separator_policy {
                langmeld_core::BlockSeparatorPolicy::Optional => "optional",
                langmeld_core::BlockSeparatorPolicy::Required => "required",
            };
            let separators = block
                .separators
                .iter()
                .map(|separator| match separator {
                    langmeld_core::BlockSeparator::Newline => "newline".to_owned(),
                    langmeld_core::BlockSeparator::Literal(value) => code_span(value),
                })
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(
                output,
                "\nAllowed in the body: {allowed}.\n\nBlock delimiters: {} and {}. Child separators: {separators} ({separator_policy}).",
                code_span(&block.open),
                code_span(&block.close)
            );
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
    let assumptions = &capabilities.editor_configuration_assumptions;
    let bracket_pairs = assumptions
        .bracket_pairs
        .iter()
        .map(|(open, close)| format!("{} → {}", code_span(open), code_span(close)))
        .collect::<Vec<_>>();
    let bracket_pairs = if bracket_pairs.is_empty() {
        "none inferred".to_owned()
    } else {
        bracket_pairs.join(", ")
    };
    let _ = writeln!(
        output,
        "## Editor configuration\n\n{}\n\nBracket pairs: {bracket_pairs}.\n",
        editor_configuration_description(assumptions)
    );
    append_capability_table(&mut output, "Generated artifacts", &capabilities.artifacts);
    append_capability_table(
        &mut output,
        "Language and host features",
        &capabilities.features,
    );
    output
}

fn append_capability_table(
    output: &mut String,
    heading: &str,
    capabilities: &BTreeMap<String, Capability>,
) {
    if capabilities.is_empty() {
        return;
    }
    let _ = writeln!(
        output,
        "## {heading}\n\n| Capability | Status | Details |\n| --- | --- | --- |"
    );
    for (name, capability) in capabilities {
        let status = match capability.status {
            CapabilityStatus::Supported => "supported",
            CapabilityStatus::Partial => "partial",
            CapabilityStatus::Customized => "customized",
            CapabilityStatus::Unavailable => "unavailable",
        };
        let details = capability
            .reason
            .as_deref()
            .map(markdown_table_cell)
            .unwrap_or_else(|| {
                "Generated or provided from the validated language model.".to_owned()
            });
        let _ = writeln!(output, "| {} | {status} | {details} |", code_span(name));
    }
    output.push('\n');
}

fn markdown_table_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
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
        FieldKind::Expression {
            grammar,
            terminator,
        } => format!(
            "expression using {} through {}",
            code_span(grammar),
            code_span(terminator)
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
