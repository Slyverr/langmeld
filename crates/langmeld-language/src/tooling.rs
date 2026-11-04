use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

use langmeld_core::{FieldKind, FieldSpec, LanguageSpec, LexicalSpec, ValidatedLanguageSpec};

use crate::{
    CLASSIFICATION_TYPES, Capability, EditorConfigurationAssumptions, PatternCompileError,
    PreparedLanguage, ToolingCapabilities, generate_tree_sitter, rust_codegen::generate_rust_ast,
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
        json_value(language_configuration(spec))?,
    );
    artifacts.insert("agent-skill/SKILL.md".to_owned(), agent_skill(spec, &slug));
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
        Ok(artifacts) => (Capability::supported(), Some(artifacts)),
        Err(reason) => (Capability::unavailable(reason), None),
    }
}

fn capability_report(
    language: &ValidatedLanguageSpec,
    prepared: &PreparedLanguage,
    tree_sitter: Capability,
) -> ToolingCapabilities {
    let spec = language.spec();
    let default_lexical = LexicalSpec::default();
    let lexical = spec.lexical.as_ref().unwrap_or(&default_lexical);
    let has_symbols = spec
        .constructs
        .iter()
        .any(|construct| construct.symbol.is_some());
    let has_contextual_fields = spec
        .constructs
        .iter()
        .flat_map(|construct| &construct.fields)
        .any(|field| kind_has_enum(&field.kind) || kind_has_reference(&field.kind));
    let has_hover = spec
        .constructs
        .iter()
        .any(|construct| !construct.fields.is_empty() || construct.symbol.is_some());
    let has_custom_parsers = spec
        .constructs
        .iter()
        .any(|construct| construct.custom_parser.is_some());
    let artifacts = BTreeMap::from([
        ("agent_skill".to_owned(), Capability::supported()),
        ("language_configuration".to_owned(), Capability::supported()),
        ("language_metadata".to_owned(), Capability::supported()),
        ("reference_document".to_owned(), Capability::supported()),
        (
            "textmate_grammar".to_owned(),
            Capability::partial(
                "TextMate highlighting is lexical and cannot provide parser-aware or semantic highlighting.",
            ),
        ),
        ("tree_sitter_grammar".to_owned(), tree_sitter),
        ("typed_rust_ast".to_owned(), Capability::supported()),
    ]);
    let mut features = BTreeMap::from([
        (
            "host.domain_lowering".to_owned(),
            Capability::customized(
                "Domain lowering is language-specific and must be implemented by the Rust host.",
            ),
        ),
        (
            "host.execution".to_owned(),
            Capability::customized(
                "Execution semantics and runtime state are owned by the Rust host.",
            ),
        ),
        (
            "host.type_system".to_owned(),
            Capability::customized(
                "Language-specific type rules require host-provided analysis callbacks.",
            ),
        ),
        (
            "language.embedded_completion_providers".to_owned(),
            Capability::customized(
                "Host applications can register typed completion providers for language-specific suggestions.",
            ),
        ),
        (
            "language.embedded_lsp_server".to_owned(),
            Capability::supported(),
        ),
        (
            "language.source_formatter".to_owned(),
            if has_custom_parsers {
                Capability::customized(
                    "Custom parser syntax requires a host-provided formatter for complete formatting.",
                )
            } else {
                Capability::supported()
            },
        ),
        ("lsp.document_symbols".to_owned(), Capability::supported()),
        (
            "lsp.hover".to_owned(),
            if has_hover {
                Capability::supported()
            } else {
                Capability::unavailable(
                    "The language declares no fields or symbols for schema-based hover information.",
                )
            },
        ),
        (
            "lsp.open_document_diagnostics".to_owned(),
            Capability::supported(),
        ),
        (
            "lsp.ranged_document_changes".to_owned(),
            Capability::supported(),
        ),
        ("lsp.semantic_tokens".to_owned(), Capability::supported()),
        (
            "lsp.parser_continuation_completion".to_owned(),
            Capability::partial(
                "Parser-derived continuations cover unambiguous literal and field sequences; optional, choice, and repeated patterns are not completed.",
            ),
        ),
        (
            "parser.custom_construct_callbacks".to_owned(),
            Capability::customized(
                "Host Rust callbacks can implement constructs that do not fit declarative syntax patterns.",
            ),
        ),
        (
            "parser.declarative_patterns".to_owned(),
            Capability::supported(),
        ),
        (
            "lsp.project_definition_reload".to_owned(),
            Capability::partial(
                "Project mode reloads valid definition changes when the editor client supports dynamic watched-file registration.",
            ),
        ),
    ]);
    features.insert(
        "lsp.literal_keyword_completion".to_owned(),
        if prepared.keywords().next().is_some() {
            Capability::supported()
        } else {
            Capability::unavailable("The language has no literal syntax keywords to suggest.")
        },
    );
    features.insert(
        "lsp.contextual_completion".to_owned(),
        if has_contextual_fields {
            Capability::supported()
        } else {
            Capability::unavailable(
                "The language declares no enum or reference fields for schema-based suggestions.",
            )
        },
    );
    features.insert(
        "lsp.symbol_navigation".to_owned(),
        if has_symbols {
            Capability::partial(
                "Generic navigation and rename cover resolved declarations in currently open documents.",
            )
        } else {
            Capability::unavailable("The language declares no symbol metadata.")
        },
    );

    ToolingCapabilities {
        schema_version: 2,
        language: spec.name.clone(),
        language_version: spec.version.clone(),
        root: spec.root.clone(),
        source_extensions: spec.source_extensions.clone(),
        artifacts,
        features,
        editor_configuration_assumptions: EditorConfigurationAssumptions {
            line_comment: lexical.line_comment.clone(),
            block_comment: lexical
                .block_comment
                .as_ref()
                .map(|comment| [comment.open.clone(), comment.close.clone()]),
            bracket_pairs: editor_bracket_pairs(spec),
            reason: "Comment settings follow the validated lexical policy; bracket pairs are inferred from declared blocks and built-in list, map, property, and expression fields.",
        },
    }
}

fn json_file(spec: &LanguageSpec) -> Result<String, serde_json::Error> {
    json_value(serde_json::to_value(spec)?)
}

fn json_value(value: serde_json::Value) -> Result<String, serde_json::Error> {
    Ok(serde_json::to_string_pretty(&value)? + "\n")
}

fn language_configuration(spec: &LanguageSpec) -> serde_json::Value {
    let brackets = editor_bracket_pairs(spec);
    let mut auto_closing_pairs = brackets.clone();
    auto_closing_pairs.insert(("\"".to_owned(), "\"".to_owned()));
    let default_lexical = LexicalSpec::default();
    let lexical = spec.lexical.as_ref().unwrap_or(&default_lexical);
    let mut comments = serde_json::Map::new();
    if let Some(line_comment) = &lexical.line_comment {
        comments.insert("lineComment".to_owned(), serde_json::json!(line_comment));
    }
    if let Some(block_comment) = &lexical.block_comment {
        comments.insert(
            "blockComment".to_owned(),
            serde_json::json!([block_comment.open, block_comment.close]),
        );
    }
    serde_json::json!({
        "comments": comments,
        "brackets": brackets,
        "autoClosingPairs": auto_closing_pairs
    })
}

fn editor_bracket_pairs(spec: &LanguageSpec) -> BTreeSet<(String, String)> {
    let mut brackets = BTreeSet::new();
    for construct in &spec.constructs {
        if let Some(block) = &construct.block {
            brackets.insert((block.open.clone(), block.close.clone()));
        }
        for field in &construct.fields {
            collect_field_bracket_pairs(&field.kind, &mut brackets);
        }
    }
    for record in &spec.types {
        for field in &record.fields {
            collect_field_bracket_pairs(&field.kind, &mut brackets);
        }
    }
    brackets
}

fn collect_field_bracket_pairs(kind: &FieldKind, output: &mut BTreeSet<(String, String)>) {
    match kind {
        FieldKind::Properties { fields, .. } => {
            output.insert(("{".to_owned(), "}".to_owned()));
            for field in fields {
                collect_field_bracket_pairs(&field.kind, output);
            }
        }
        FieldKind::List { item } => {
            output.insert(("[".to_owned(), "]".to_owned()));
            collect_field_bracket_pairs(item, output);
        }
        FieldKind::Map { value, .. } => {
            output.insert(("{".to_owned(), "}".to_owned()));
            collect_field_bracket_pairs(value, output);
        }
        FieldKind::Expression { .. } => {
            output.insert(("(".to_owned(), ")".to_owned()));
        }
        FieldKind::Identifier
        | FieldKind::Number
        | FieldKind::Text
        | FieldKind::Enum { .. }
        | FieldKind::NamedEnum { .. }
        | FieldKind::Reference { .. } => {}
    }
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
        "---\nname: {slug}-language\ndescription: Guidance for source written in the {slug} language.\n---\n\n# {title} language\n\nUse this guidance when creating or editing source for `{title}` version {}. {suffixes}\n\n## Workflow\n\n1. Read [the grammar reference](references/grammar.md) and [the current capabilities](references/capabilities.json).\n2. Use the declared syntax, fields, constraints, and construct relationships.\n3. Use `langmeld check` or the initialized host CLI to validate source. The standalone LSP provides diagnostics, versioned full/ranged document changes, literal-keyword suggestions, schema-based enum and in-scope reference suggestions, parser-derived continuations for simple missing-field patterns, document symbols, schema-based hover, semantic tokens, generic definition/references, and safe rename for resolved declarations across open documents. Rust hosts can embed `langmeld_lsp::serve_configured` with registered parser, analysis, and typed completion callbacks; host-owned symbol resolvers do not expose generic rename. The standalone binary does not load host code.\n4. Resolve diagnostics before treating a source file as valid.\n5. Run the host application's own renderer, compiler, or interpreter to check domain behavior; Langmeld does not provide generic execution or domain lowering.\n\n## Generated artifacts\n\nThe generated TextMate grammar is lexical and the language configuration mirrors declared comment delimiters while retaining documented conventional bracket defaults. Neither provides parser-backed editor behavior. The generic LSP offers global literal-keyword completion, captured-value enum/reference suggestions, parser-derived continuations for simple literal/capture patterns, simple child-construct snippets, document symbols, schema-based hover, generic definition/references, and safe rename for resolved symbols across open documents; it does not load unopened project source files or infer host-specific semantics. Patterns with optional, choice, or repeated sections do not receive continuation snippets.\n",
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
        | FieldKind::Reference { .. }
        | FieldKind::Expression { .. } => {}
    }
}

fn kind_has_enum(kind: &FieldKind) -> bool {
    match kind {
        FieldKind::Enum { .. } | FieldKind::NamedEnum { .. } => true,
        FieldKind::Properties { fields, .. } => {
            fields.iter().any(|field| kind_has_enum(&field.kind))
        }
        FieldKind::List { item } => kind_has_enum(item),
        FieldKind::Map { value, .. } => kind_has_enum(value),
        FieldKind::Identifier
        | FieldKind::Number
        | FieldKind::Text
        | FieldKind::Reference { .. }
        | FieldKind::Expression { .. } => false,
    }
}

fn kind_has_reference(kind: &FieldKind) -> bool {
    match kind {
        FieldKind::Reference { .. } => true,
        FieldKind::Properties { fields, .. } => {
            fields.iter().any(|field| kind_has_reference(&field.kind))
        }
        FieldKind::List { item } => kind_has_reference(item),
        FieldKind::Map { value, .. } => kind_has_reference(value),
        FieldKind::Identifier
        | FieldKind::Number
        | FieldKind::Text
        | FieldKind::Enum { .. }
        | FieldKind::NamedEnum { .. }
        | FieldKind::Expression { .. } => false,
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
        | FieldKind::Reference { .. }
        | FieldKind::Expression { .. } => {}
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
    for expression in &spec.expressions {
        operators.extend(
            expression
                .operators
                .iter()
                .map(|operator| operator.token.clone()),
        );
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

fn textmate_grammar(
    slug: &str,
    keywords: &BTreeSet<String>,
    operators: &BTreeSet<String>,
    enum_values: &BTreeSet<String>,
    lexical: &LexicalSpec,
) -> serde_json::Value {
    let mut patterns = Vec::new();
    if let Some(line_comment) = &lexical.line_comment {
        patterns.push(serde_json::json!({
            "name": "comment.line",
            "match": format!("{}[^\\r\\n]*", regex_escape(line_comment))
        }));
    }
    if let Some(block_comment) = &lexical.block_comment {
        patterns.push(serde_json::json!({
            "name": "comment.block",
            "begin": regex_escape(&block_comment.open),
            "end": regex_escape(&block_comment.close)
        }));
    }
    patterns.extend([
        serde_json::json!({"name": "string.quoted.double", "begin": "\\\"", "end": "\\\""}),
        serde_json::json!({"name": "constant.numeric", "match": "\\b-?[0-9]+(\\.[0-9]+)?\\b"}),
    ]);
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
