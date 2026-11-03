use langmeld_core::{
    ConstructBuilder, ConstructSpec, Diagnostic, FieldSpec, ScopeLookup, ScopeSpec, SourceFile,
    SymbolSpec,
};
use std::collections::BTreeMap;

use super::{fields::convert_field_declarations, helpers::*};
use crate::{AstNode, Value};

fn convert_block(node: &AstNode) -> Result<Vec<String>, Vec<Diagnostic>> {
    let Some(values) = node.fields.get("allowed") else {
        return Ok(Vec::new());
    };
    let Value::List(values) = &values.value else {
        return Err(vec![error(
            node,
            "block declaration child list did not parse as a list",
        )]);
    };
    let allowed = values
        .iter()
        .map(|value| match &value.value {
            Value::Identifier(value) => Ok(value.clone()),
            _ => Err(vec![error(node, "block child IDs must be identifiers")]),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(allowed)
}

pub(super) fn convert_construct(
    node: &AstNode,
    enums: &BTreeMap<String, Vec<String>>,
    named_types: &BTreeMap<String, Vec<FieldSpec>>,
    properties: &BTreeMap<String, Vec<FieldSpec>>,
    source: &SourceFile,
) -> Result<ConstructSpec, Vec<Diagnostic>> {
    let id = identifier(node, "id")?;
    let custom_parser_node = optional_child(node, "custom_parser_decl")?;
    let syntax_blocks: Vec<_> = node
        .children
        .iter()
        .filter(|child| child.construct == "syntax_block")
        .collect();
    let syntax_lists: Vec<_> = node
        .children
        .iter()
        .filter(|child| child.construct == "syntax_list_decl")
        .collect();
    if !syntax_blocks.is_empty() && !syntax_lists.is_empty() {
        return Err(vec![error(
            node,
            "named syntax blocks and `syntax: [...]` cannot be mixed",
        )]);
    }
    let mut syntax = Vec::new();
    let mut syntax_ids = Vec::new();
    let mut named_variants = None;
    if !syntax_blocks.is_empty() {
        for syntax_node in syntax_blocks {
            for variant in &syntax_node.children {
                if variant.construct != "syntax_variant" {
                    return Err(vec![error(
                        variant,
                        format!(
                            "unsupported child `{}` in syntax declaration",
                            variant.construct
                        ),
                    )]);
                }
                let variant_id = optional_identifier(variant, "id")?;
                let current_is_named = variant_id.is_some();
                if named_variants.is_some_and(|is_named| is_named != current_is_named) {
                    return Err(vec![error(
                        variant,
                        "named and unnamed syntax patterns cannot be mixed",
                    )]);
                }
                named_variants = Some(current_is_named);
                if let Some(variant_id) = variant_id {
                    syntax_ids.push(variant_id.to_owned());
                }
                syntax.push(text(variant, "pattern")?);
            }
        }
    } else if !syntax_lists.is_empty() {
        for syntax_node in syntax_lists {
            syntax.extend(texts(syntax_node, "patterns")?);
        }
    }

    let mut fields = Vec::new();
    for fields_node in node
        .children
        .iter()
        .filter(|child| child.construct == "fields_block")
    {
        fields.extend(convert_field_declarations(
            fields_node,
            enums,
            named_types,
            properties,
            source,
        )?);
    }

    let block = optional_child(node, "block_declaration")?
        .map(convert_block)
        .transpose()?;
    let symbol = optional_child(node, "symbol_block")?
        .map(convert_symbol)
        .transpose()?;
    let implements = optional_child(node, "implements_declaration")?
        .map(convert_implements)
        .transpose()?
        .unwrap_or_default();
    let custom_parser = custom_parser_node
        .map(|node| identifier(node, "id"))
        .transpose()?;

    if !syntax_ids.is_empty() && syntax_ids.len() != syntax.len() {
        return Err(vec![Diagnostic::error(
            "LM1021",
            format!("construct `{id}` must provide one syntax id per pattern"),
            node.span,
        )]);
    }

    let mut construct = ConstructBuilder::new(id).implements(implements);
    if syntax_ids.is_empty() {
        for pattern in syntax {
            construct = construct.syntax(pattern);
        }
    } else {
        for (syntax_id, pattern) in syntax_ids.into_iter().zip(syntax) {
            construct = construct.syntax_variant(syntax_id, pattern);
        }
    }
    for field in fields {
        construct = construct.field(field);
    }
    if let Some(block) = block {
        construct = construct.block(block);
    }
    if let Some(symbol) = symbol {
        construct = construct.symbol(symbol);
    }
    if let Some(parser) = custom_parser {
        construct = construct.custom_parser(parser);
    }
    Ok(construct.into())
}

fn convert_implements(node: &AstNode) -> Result<Vec<String>, Vec<Diagnostic>> {
    match (node.fields.get("category"), node.fields.get("categories")) {
        (Some(category), None) => match &category.value {
            Value::Identifier(category) => Ok(vec![category.clone()]),
            _ => Err(vec![error(
                node,
                "implemented category must be an identifier",
            )]),
        },
        (None, Some(categories)) => {
            let Value::List(categories) = &categories.value else {
                return Err(vec![error(node, "implemented categories must be a list")]);
            };
            categories
                .iter()
                .map(|category| match &category.value {
                    Value::Identifier(category) => Ok(category.clone()),
                    _ => Err(vec![error(
                        node,
                        "implemented category names must be identifiers",
                    )]),
                })
                .collect()
        }
        (None, None) => Err(vec![error(
            node,
            "implements declaration is missing its category name",
        )]),
        (Some(_), Some(_)) => Err(vec![error(
            node,
            "implements declaration cannot mix a single category with a list",
        )]),
    }
}

fn convert_symbol(node: &AstNode) -> Result<SymbolSpec, Vec<Diagnostic>> {
    for child in &node.children {
        if !matches!(
            child.construct.as_str(),
            "symbol_kind"
                | "symbol_name"
                | "symbol_scope"
                | "symbol_scope_name"
                | "symbol_scope_lookup"
        ) {
            return Err(vec![error(
                child,
                format!("unsupported symbol property `{}`", child.construct),
            )]);
        }
    }
    let kind = identifier(child(node, "symbol_kind")?, "value")?;
    let name_field = identifier(child(node, "symbol_name")?, "value")?;
    let scope_construct = optional_child(node, "symbol_scope")?
        .map(|node| identifier(node, "value"))
        .transpose()?;
    let scope_name_field = optional_child(node, "symbol_scope_name")?
        .map(|node| identifier(node, "value"))
        .transpose()?;
    let scope_lookup = optional_child(node, "symbol_scope_lookup")?
        .map(|node| identifier(node, "value"))
        .transpose()?
        .unwrap_or_else(|| "exact".to_owned());
    let lookup = match scope_lookup.as_str() {
        "exact" => ScopeLookup::Exact,
        "lexical_parents" => ScopeLookup::LexicalParents,
        other => {
            return Err(vec![error(
                node,
                format!("unsupported symbol scope lookup `{other}`"),
            )]);
        }
    };
    let scope = match (scope_construct, scope_name_field) {
        (None, None) if lookup == ScopeLookup::Exact => None,
        (Some(construct), Some(name_field)) => {
            Some(ScopeSpec::new(construct, name_field).lookup(lookup))
        }
        _ => {
            return Err(vec![error(
                node,
                "symbol scope requires both `scope` and `scope_name`; non-exact lookup requires a scope",
            )]);
        }
    };
    let symbol = SymbolSpec::new(kind, name_field);
    Ok(match scope {
        Some(scope) => symbol.with_scope(scope),
        None => symbol,
    })
}
