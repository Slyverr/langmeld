use std::collections::HashSet;

use langmeld_core::{Diagnostic, SourceFile};

use crate::{CapturedValue, ConstructSpec, LexicalRules, ParsedConstruct};

use super::{decode_definition_string as decode_quoted, unit::ImportDirective};

mod pattern;

use pattern::{lower_completion_kind, lower_pattern, validate_pattern_group_names};

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

    let mut variants = Vec::new();
    let mut variant_names = HashSet::new();
    for syntax in syntax_nodes {
        let variant_name = if syntax.fields.iter().any(|field| field.name == "name") {
            let Some(name) = string_field(syntax, "name").and_then(decode_quoted) else {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-034",
                    "syntax variant name must be a valid quoted string",
                    source,
                    syntax.span,
                ));
                return None;
            };
            if name.trim().is_empty() {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-034",
                    "syntax variant name cannot be empty",
                    source,
                    syntax.span,
                ));
                return None;
            }
            if !variant_names.insert(name.clone()) {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-035",
                    format!("syntax variant `{name}` is declared more than once"),
                    source,
                    syntax.span,
                ));
                return None;
            }
            Some(name)
        } else {
            None
        };

        if !validate_pattern_group_names(&syntax.children, source, diagnostics) {
            return None;
        }

        let mut pattern = Vec::new();
        for part in &syntax.children {
            if let Some(pattern_part) = lower_pattern(part, source, diagnostics) {
                pattern.push(pattern_part);
            }
        }
        variants.push((variant_name, pattern));
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
        .filter(|child| matches!(child.name.as_str(), "block" | "optional_block"))
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

    let mut variants = variants.into_iter();
    let (first_variant_name, first_pattern) = variants.next().unwrap_or_default();
    let mut construct = ConstructSpec::new(name, first_pattern);
    if let Some(variant_name) = first_variant_name {
        construct = construct.with_first_variant_name(variant_name);
    }
    if let Some(kind) = completion_kind {
        construct = construct.with_completion_kind(kind);
    }
    for (variant_name, pattern) in variants {
        construct = if let Some(variant_name) = variant_name {
            construct.variant_named(variant_name, pattern)
        } else {
            construct.variant(pattern)
        };
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
        construct = if block.name == "optional_block" {
            construct.optional_block_names_with_categories(open, close, children, categories)
        } else {
            construct.block_names_with_categories(open, close, children, categories)
        };
    }
    Some(construct)
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
