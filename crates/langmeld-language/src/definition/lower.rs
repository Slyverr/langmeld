use langmeld_core::{Diagnostic, SourceFile};

use crate::{
    CapturedValue, CompletionKind, ConstructSpec, FieldKind, LexicalRules, ParsedConstruct,
    PatternPart,
};

use super::{decode_definition_string as decode_quoted, unit::ImportDirective};

pub(super) fn lower_lexical_rules(
    node: &ParsedConstruct,
    source: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
) -> LexicalRules {
    let mut rules = LexicalRules::default();
    for declaration in &node.children {
        match declaration.name.as_str() {
            "line_comment" => {
                if let Some(marker) = string_field(declaration, "marker").and_then(decode_quoted) {
                    rules = rules.line_comment(marker);
                } else {
                    diagnostics.push(Diagnostic::error(
                        "LM-DEF-025",
                        "`line_comment` requires a valid quoted marker",
                        source,
                        declaration.span,
                    ));
                }
            }
            "block_comment" => {
                if let (Some(open), Some(close)) = (
                    string_field(declaration, "open").and_then(decode_quoted),
                    string_field(declaration, "close").and_then(decode_quoted),
                ) {
                    rules = rules.block_comment(open, close);
                } else {
                    diagnostics.push(Diagnostic::error(
                        "LM-DEF-026",
                        "`block_comment` requires quoted opening and closing markers",
                        source,
                        declaration.span,
                    ));
                }
            }
            _ => {}
        }
    }
    rules
}

pub(super) fn lower_import(
    node: &ParsedConstruct,
    source: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ImportDirective> {
    let Some(path) = string_field(node, "path").and_then(decode_quoted) else {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-017",
            "import requires a valid quoted path",
            source,
            node.span,
        ));
        return None;
    };
    if path.trim().is_empty() {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-017",
            "import path cannot be empty",
            source,
            node.span,
        ));
        return None;
    }
    Some(ImportDirective {
        path,
        span: node.span,
    })
}

pub(super) fn lower_construct(
    node: &ParsedConstruct,
    source: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ConstructSpec> {
    let Some(name) = string_field(node, "name").and_then(decode_quoted) else {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-003",
            "construct declaration requires a valid quoted name",
            source,
            node.span,
        ));
        return None;
    };

    let syntax_nodes: Vec<_> = node
        .children
        .iter()
        .filter(|child| child.name == "syntax")
        .collect();
    if syntax_nodes.is_empty() {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-005",
            format!("construct `{name}` must have at least one `syntax` block"),
            source,
            node.span,
        ));
        return None;
    }

    let mut patterns = Vec::new();
    for syntax in syntax_nodes {
        let mut pattern = Vec::new();
        for part in &syntax.children {
            if let Some(pattern_part) = lower_pattern(part, source, diagnostics) {
                pattern.push(pattern_part);
            }
        }
        patterns.push(pattern);
    }

    let completion_nodes: Vec<_> = node
        .children
        .iter()
        .filter(|child| child.name == "completion")
        .collect();
    if completion_nodes.len() > 1 {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-029",
            format!("construct `{name}` may declare only one completion kind"),
            source,
            completion_nodes[1].span,
        ));
        return None;
    }
    let completion_kind = completion_nodes.first().and_then(|completion| {
        lower_completion_kind(completion, source, diagnostics, "LM-DEF-031", "")
    });
    if !completion_nodes.is_empty() && completion_kind.is_none() {
        return None;
    }

    let block_nodes: Vec<_> = node
        .children
        .iter()
        .filter(|child| child.name == "block")
        .collect();
    if block_nodes.len() > 1 {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-007",
            format!("construct `{name}` may declare only one block"),
            source,
            block_nodes[1].span,
        ));
        return None;
    }

    let mut patterns = patterns.into_iter();
    let mut construct = ConstructSpec::new(name, patterns.next().unwrap_or_default());
    if let Some(kind) = completion_kind {
        construct = construct.with_completion_kind(kind);
    }
    for pattern in patterns {
        construct = construct.variant(pattern);
    }
    let mut implemented_categories = Vec::new();
    for implementation in node
        .children
        .iter()
        .filter(|child| child.name == "implements")
    {
        if let Some(category) = string_field(implementation, "category").and_then(decode_quoted) {
            implemented_categories.push(category);
        } else {
            diagnostics.push(Diagnostic::error(
                "LM-DEF-022",
                "`implements` requires a valid quoted category name",
                source,
                implementation.span,
            ));
        }
    }
    construct = construct.implements(
        &implemented_categories
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    if let Some(block) = block_nodes.first() {
        let (Some(open), Some(close)) = (
            string_field(block, "open").and_then(decode_quoted),
            string_field(block, "close").and_then(decode_quoted),
        ) else {
            diagnostics.push(Diagnostic::error(
                "LM-DEF-008",
                "block requires quoted opening and closing tokens",
                source,
                block.span,
            ));
            return None;
        };
        let mut children = Vec::new();
        let mut categories = Vec::new();
        for child in block.children.iter().filter(|child| child.name == "allow") {
            let name = string_field(child, "name").and_then(decode_quoted);
            if let Some(name) = name {
                children.push(name);
            } else {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-009",
                    "`allow` requires a valid quoted construct name",
                    source,
                    child.span,
                ));
            }
        }
        for child in block
            .children
            .iter()
            .filter(|child| child.name == "allow_category")
        {
            if let Some(category) = string_field(child, "category").and_then(decode_quoted) {
                categories.push(category);
            } else {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-023",
                    "`allow_category` requires a valid quoted category name",
                    source,
                    child.span,
                ));
            }
        }
        construct = construct.block_names_with_categories(open, close, children, categories);
    }
    Some(construct)
}

pub(super) fn lower_pattern(
    node: &ParsedConstruct,
    source: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<PatternPart> {
    match node.name.as_str() {
        "literal" | "token" | "identifier" | "integer" | "string" => {
            let Some(value) = string_field(node, "value").and_then(decode_quoted) else {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-006",
                    "syntax item requires a valid quoted value",
                    source,
                    node.span,
                ));
                return None;
            };
            Some(match node.name.as_str() {
                "literal" => PatternPart::literal(value),
                "token" => PatternPart::token(value),
                "identifier" => PatternPart::Field {
                    name: value,
                    kind: FieldKind::Identifier,
                },
                "integer" => PatternPart::Field {
                    name: value,
                    kind: FieldKind::Integer,
                },
                "string" => PatternPart::Field {
                    name: value,
                    kind: FieldKind::String,
                },
                _ => return None,
            })
        }
        "choice" => {
            let Some(name) = string_field(node, "name").and_then(decode_quoted) else {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-006",
                    "choice syntax requires a field name",
                    source,
                    node.span,
                ));
                return None;
            };
            let mut values = Vec::new();
            for value in node
                .children
                .iter()
                .filter(|child| child.name == "choice_value")
            {
                if let Some(value) = string_field(value, "value").and_then(decode_quoted) {
                    values.push(value);
                } else {
                    diagnostics.push(Diagnostic::error(
                        "LM-DEF-006",
                        "choice values must be valid quoted tokens",
                        source,
                        value.span,
                    ));
                }
            }
            if values.is_empty() {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-028",
                    "choice syntax requires at least one `value` declaration",
                    source,
                    node.span,
                ));
                return None;
            }
            let completion_nodes: Vec<_> = node
                .children
                .iter()
                .filter(|child| child.name == "completion")
                .collect();
            if completion_nodes.len() > 1 {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-032",
                    "a choice field may declare only one completion kind",
                    source,
                    completion_nodes[1].span,
                ));
                return None;
            }
            let completion_kind = completion_nodes.first().and_then(|completion| {
                lower_completion_kind(
                    completion,
                    source,
                    diagnostics,
                    "LM-DEF-033",
                    " for choice field",
                )
            });
            if !completion_nodes.is_empty() && completion_kind.is_none() {
                return None;
            }
            Some(if let Some(kind) = completion_kind {
                PatternPart::choice_with_completion_kind(name, values, kind)
            } else {
                PatternPart::choice(name, values)
            })
        }
        "expression" => {
            let (Some(name), Some(terminator)) = (
                string_field(node, "name").and_then(decode_quoted),
                string_field(node, "terminator").and_then(decode_quoted),
            ) else {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-006",
                    "expression syntax requires a field name and a valid quoted terminator",
                    source,
                    node.span,
                ));
                return None;
            };
            Some(PatternPart::expression(name, terminator))
        }
        "optional" | "zero_or_more" | "one_or_more" => {
            let parts = node
                .children
                .iter()
                .filter_map(|child| lower_pattern(child, source, diagnostics))
                .collect::<Vec<_>>();
            Some(match node.name.as_str() {
                "optional" => PatternPart::optional(parts),
                "zero_or_more" => PatternPart::zero_or_more(parts),
                "one_or_more" => PatternPart::one_or_more(parts),
                _ => return None,
            })
        }
        _ => None,
    }
}

fn lower_completion_kind(
    node: &ParsedConstruct,
    source: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
    unknown_code: &'static str,
    unknown_suffix: &str,
) -> Option<CompletionKind> {
    let Some(value) = completion_kind_value(node) else {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-030",
            "completion requires a valid kind name",
            source,
            node.span,
        ));
        return None;
    };
    let Some(kind) = parse_completion_kind(&value) else {
        diagnostics.push(Diagnostic::error(
            unknown_code,
            format!("unknown completion kind `{value}`{unknown_suffix}"),
            source,
            node.span,
        ));
        return None;
    };
    Some(kind)
}

pub(super) fn string_field<'a>(node: &'a ParsedConstruct, name: &str) -> Option<&'a str> {
    node.fields
        .iter()
        .find(|field| field.name == name)
        .and_then(|field| match &field.value {
            CapturedValue::String(value) => Some(value.as_str()),
            _ => None,
        })
}

fn parse_completion_kind(value: &str) -> Option<CompletionKind> {
    CompletionKind::from_name(value)
}

fn completion_kind_value(node: &ParsedConstruct) -> Option<String> {
    node.fields
        .iter()
        .find(|field| field.name == "kind")
        .and_then(|field| match &field.value {
            CapturedValue::String(value) => decode_quoted(value),
            CapturedValue::Token {
                text,
                kind: crate::TokenKind::Identifier,
            } => Some(text.clone()),
            _ => None,
        })
}
