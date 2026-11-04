use langmeld_core::{
    BehaviorStage, Diagnostic, ExpressionAssociativity, ExpressionAtomKind, ExpressionOperatorSpec,
    ExpressionSpec, FieldSpec, InfixOperatorSpec, LanguageBuilder, LanguageSpec, LexicalSpec,
    SourceFile, Span, ValidatedLanguageSpec,
};
use std::collections::{BTreeMap, VecDeque};

use super::{
    constructs::convert_construct,
    fields::{convert_field_declarations, optional_integer},
    helpers::*,
};
use crate::{AstNode, Value};

pub(super) fn convert_language(
    node: &AstNode,
    source: &SourceFile,
) -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    convert_language_modules(std::iter::once((node, source)))
}

pub(crate) fn convert_language_modules<'a>(
    modules: impl IntoIterator<Item = (&'a AstNode, &'a SourceFile)>,
) -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    let modules: Vec<_> = modules.into_iter().collect();
    let root_span = modules.last().map_or_else(
        || {
            Err(vec![Diagnostic::error(
                "LMBOOT001",
                "definition project contains no language modules",
                Span::new(0, 0),
            )])
        },
        |(root, _)| Ok(root.span),
    )?;
    convert_language_module_specs(modules)?
        .validate()
        .map_err(|errors| {
            errors
                .into_iter()
                .map(|issue| Diagnostic::error(issue.code, issue.message, root_span))
                .collect()
        })
}

pub(crate) fn convert_language_module_specs<'a>(
    modules: impl IntoIterator<Item = (&'a AstNode, &'a SourceFile)>,
) -> Result<LanguageSpec, Vec<Diagnostic>> {
    let modules: Vec<_> = modules.into_iter().collect();
    let Some((root, root_source)) = modules.last() else {
        return Err(vec![Diagnostic::error(
            "LMBOOT001",
            "definition project contains no language modules",
            Span::new(0, 0),
        )]);
    };
    if root.syntax_variant != "root" {
        return Err(vec![error(
            root,
            "the root language definition must declare `language {name}` metadata",
        )]);
    }
    for (module, source) in modules.iter().take(modules.len().saturating_sub(1)) {
        if module.construct != "language" {
            return Err(vec![error(module, "expected a `language` declaration")]);
        }
        if module.syntax_variant == "constructs" {
            if let Some(declaration) = module
                .children
                .iter()
                .find(|child| !matches!(child.construct.as_str(), "construct" | "import_decl"))
            {
                return Err(vec![error(
                    declaration,
                    "constructs modules may contain only `construct` declarations and imports",
                )]);
            }
            continue;
        }
        if module.syntax_variant == "types" {
            if let Some(declaration) = module.children.iter().find(|child| {
                !matches!(
                    child.construct.as_str(),
                    "enum_decl" | "record_type" | "properties_bundle" | "import_decl"
                )
            }) {
                return Err(vec![error(
                    declaration,
                    "types modules may contain only enum, record-type, properties, and imports",
                )]);
            }
            continue;
        }
        if module.syntax_variant == "expressions" {
            if let Some(declaration) = module.children.iter().find(|child| {
                !matches!(child.construct.as_str(), "expression_decl" | "import_decl")
            }) {
                return Err(vec![error(
                    declaration,
                    "expressions modules may contain only `expression` declarations and imports",
                )]);
            }
            continue;
        }
        if identifier(module, "name")? != identifier(root, "name")?
            || text(child(module, "language_version")?, "value")?
                != text(child(root, "language_version")?, "value")?
            || identifier(child(module, "language_root")?, "value")?
                != identifier(child(root, "language_root")?, "value")?
            || schema_version(module, source)? != schema_version(root, root_source)?
        {
            return Err(vec![error(
                module,
                "imported definition targets a different language name, version, or root",
            )]);
        }
    }
    let name = identifier(root, "name")?;
    let version = text(child(root, "language_version")?, "value")?;
    let root_id = identifier(child(root, "language_root")?, "value")?;
    let schema_version = schema_version(root, root_source)?;
    let mut enums = BTreeMap::new();
    let mut named_types = BTreeMap::new();
    let mut properties = BTreeMap::new();
    let mut categories = Vec::new();
    let mut rules = Vec::new();
    let mut source_extensions = Vec::new();
    let mut behaviors = Vec::new();
    let mut types = Vec::new();
    let mut expressions = Vec::new();
    let mut constructs = Vec::new();
    let mut lexical = None;
    let declarations: Vec<_> = modules
        .iter()
        .flat_map(|(module, source)| module.children.iter().map(move |node| (node, *source)))
        .collect();

    // Collect every type name before resolving any uses. Files in the conventional directories
    // are ordered for reproducibility, not to impose a dependency order on the author.
    let mut declared_type_ids = BTreeMap::new();
    for (declaration, _) in &declarations {
        if matches!(
            declaration.construct.as_str(),
            "enum_decl" | "record_type" | "properties_bundle"
        ) {
            let id = identifier(declaration, "id")?;
            let kind = match declaration.construct.as_str() {
                "enum_decl" => "enum",
                "record_type" => "record type",
                "properties_bundle" => "property bundle",
                _ => unreachable!(),
            };
            if let Some(existing_kind) = declared_type_ids.get(&id) {
                let message = if *existing_kind == kind {
                    format!("duplicate {kind} declaration `{id}`")
                } else {
                    let existing_article = if *existing_kind == "enum" { "an" } else { "a" };
                    let new_article = if kind == "enum" { "an" } else { "a" };
                    format!(
                        "type `{id}` is declared as both {existing_article} {existing_kind} and {new_article} {kind}"
                    )
                };
                return Err(vec![error(declaration, message)]);
            }
            declared_type_ids.insert(id, kind);
        }
    }

    // Enums have no dependencies, so make them available to all record and property declarations.
    for (declaration, _) in &declarations {
        if declaration.construct == "enum_decl" {
            let (id, values) = convert_enum(declaration)?;
            enums.insert(id, values);
        }
    }

    // Record schemas and property bundles can refer to one another. Retry only declarations
    // waiting on a type that just became available; rescanning every unresolved declaration on
    // every pass makes long forward-reference chains quadratic.
    let mut pending: VecDeque<_> = declarations
        .iter()
        .enumerate()
        .filter(|(_, (node, _))| {
            matches!(node.construct.as_str(), "record_type" | "properties_bundle")
        })
        .map(|(order, (node, source))| (order, *node, *source))
        .collect();
    let mut waiting_for =
        BTreeMap::<String, Vec<(usize, &AstNode, &SourceFile, Vec<Diagnostic>)>>::new();
    while let Some((order, declaration, source)) = pending.pop_front() {
        let converted = match declaration.construct.as_str() {
            "record_type" => convert_type(declaration, &enums, &named_types, &properties, source),
            "properties_bundle" => {
                convert_properties(declaration, &enums, &named_types, &properties, source)
            }
            _ => unreachable!("only record and property declarations are pending"),
        };
        match converted {
            Ok((id, fields)) => {
                if declaration.construct == "record_type" {
                    named_types.insert(id.clone(), fields.clone());
                    types.push((id.clone(), fields));
                } else {
                    // A property bundle is both a reusable field group (`use Bundle`)
                    // and a named record when captured as a property value. Keeping it
                    // in the canonical type table preserves that identity for codegen.
                    named_types.insert(id.clone(), fields.clone());
                    types.push((id.clone(), fields.clone()));
                    properties.insert(id.clone(), fields);
                }
                if let Some(dependents) = waiting_for.remove(&id) {
                    pending.extend(
                        dependents
                            .into_iter()
                            .map(|(order, node, source, _)| (order, node, source)),
                    );
                }
            }
            Err(diagnostics) => {
                if let Some(dependency) = unresolved_declared_type(
                    &diagnostics,
                    &declared_type_ids,
                    &enums,
                    &named_types,
                    &properties,
                ) {
                    waiting_for.entry(dependency.to_owned()).or_default().push((
                        order,
                        declaration,
                        source,
                        diagnostics,
                    ));
                } else {
                    return Err(diagnostics);
                }
            }
        }
    }
    if let Some((_, declaration, _, diagnostics)) = waiting_for
        .values()
        .flatten()
        .min_by_key(|(order, _, _, _)| order)
    {
        let id = identifier(declaration, "id")?;
        return Err(vec![error(
            declaration,
            format!(
                "cyclic type dependency prevents resolving `{id}`: {}",
                diagnostics
                    .first()
                    .map_or("unresolved declaration dependency", |diagnostic| diagnostic
                        .message
                        .as_str())
            ),
        )]);
    }

    for (module, source) in modules {
        for declaration in &module.children {
            match declaration.construct.as_str() {
                "language_version" | "language_root" | "schema_version_decl" | "import_decl" => {}
                "lexical_decl" => {
                    if module.syntax_variant != "root" {
                        return Err(vec![error(
                            declaration,
                            "lexical settings must be declared in the root language file",
                        )]);
                    }
                    if lexical.is_some() {
                        return Err(vec![error(
                            declaration,
                            "language declares more than one lexical settings block",
                        )]);
                    }
                    lexical = Some(convert_lexical_spec(declaration)?);
                }
                "source_extension_decl" => {
                    source_extensions.push(text(declaration, "value")?);
                }
                "category_decl" => categories.push(identifier(declaration, "id")?),
                "behavior_decl" => behaviors.push(convert_behavior(declaration)?),
                "rule_decl" => rules.push(convert_rule(declaration)?),
                "enum_decl" | "record_type" | "properties_bundle" => {}
                "expression_decl" => expressions.push(convert_expression(declaration, source)?),
                "construct" => constructs.push(convert_construct(
                    declaration,
                    &enums,
                    &named_types,
                    &properties,
                    source,
                )?),
                unsupported => {
                    return Err(vec![error(
                        declaration,
                        format!("unsupported bootstrap declaration `{unsupported}`"),
                    )]);
                }
            }
        }
    }
    types.sort_by(|left, right| left.0.cmp(&right.0));
    let mut language = LanguageBuilder::new(name)
        .schema_version(schema_version)
        .version(version)
        .root(root_id);
    if let Some(lexical) = lexical {
        language = language.lexical(lexical);
    }
    for extension in source_extensions {
        language = language.source_extension(extension);
    }
    for category in categories {
        language = language.category(category);
    }
    for (id, parameters, pattern) in rules {
        language = language.parameterized_rule(id, parameters, pattern);
    }
    for (id, values) in enums {
        language = language.named_enum(id, values);
    }
    for (id, fields) in types {
        language = language.named_type(id, fields);
    }
    for expression in expressions {
        language = language.expression(expression);
    }
    for (id, stage) in behaviors {
        language = language.behavior(id, stage);
    }
    for construct in constructs {
        language = language.construct(construct);
    }
    Ok(language.into_spec())
}

fn convert_lexical_spec(node: &AstNode) -> Result<LexicalSpec, Vec<Diagnostic>> {
    let mut lexical = LexicalSpec::new();
    let mut has_line_comment = false;
    let mut has_block_comment = false;
    for declaration in &node.children {
        match declaration.construct.as_str() {
            "line_comment_decl" => {
                if has_line_comment {
                    return Err(vec![error(
                        declaration,
                        "lexical settings declare more than one line comment delimiter",
                    )]);
                }
                has_line_comment = true;
                lexical = lexical.line_comment(text(declaration, "marker")?);
            }
            "block_comment_decl" => {
                if has_block_comment {
                    return Err(vec![error(
                        declaration,
                        "lexical settings declare more than one block comment pair",
                    )]);
                }
                has_block_comment = true;
                lexical =
                    lexical.block_comment(text(declaration, "open")?, text(declaration, "close")?);
            }
            unsupported => {
                return Err(vec![error(
                    declaration,
                    format!("unsupported lexical declaration `{unsupported}`"),
                )]);
            }
        }
    }
    Ok(lexical)
}

fn convert_expression(
    node: &AstNode,
    source: &SourceFile,
) -> Result<ExpressionSpec, Vec<Diagnostic>> {
    let mut expression = ExpressionSpec::new(identifier(node, "id")?);
    for declaration in &node.children {
        match declaration.construct.as_str() {
            "expression_atom" => {
                let atom_kind =
                    optional_identifier_or_enum(declaration, "kind")?.ok_or_else(|| {
                        vec![error(declaration, "expression atom is missing its kind")]
                    })?;
                let atom = match atom_kind {
                    "number" => ExpressionAtomKind::Number,
                    "boolean" => ExpressionAtomKind::Boolean,
                    "identifier" => ExpressionAtomKind::Identifier,
                    other => {
                        return Err(vec![error(
                            declaration,
                            format!("unknown expression atom `{other}`"),
                        )]);
                    }
                };
                expression.atoms.push(atom);
            }
            "expression_operator" => {
                expression
                    .operators
                    .push(convert_expression_operator(declaration, source)?);
            }
            "expression_host_atom_parser" => {
                if expression.host_atom_parser.is_some() {
                    return Err(vec![error(
                        declaration,
                        "expression grammar declares more than one host atom parser",
                    )]);
                }
                expression.host_atom_parser = Some(identifier(declaration, "id")?);
            }
            unsupported => {
                return Err(vec![error(
                    declaration,
                    format!("unsupported expression declaration `{unsupported}`"),
                )]);
            }
        }
    }
    Ok(expression)
}

fn convert_expression_operator(
    node: &AstNode,
    source: &SourceFile,
) -> Result<ExpressionOperatorSpec, Vec<Diagnostic>> {
    let token = text(node, "token")?;
    let prefix_precedence = optional_integer(node, "prefix_precedence", source)?
        .map(u8::try_from)
        .transpose()
        .map_err(|_| vec![error(node, "prefix precedence must be between 0 and 255")])?;
    let infix_precedence = optional_integer(node, "infix_precedence", source)?
        .map(u8::try_from)
        .transpose()
        .map_err(|_| vec![error(node, "infix precedence must be between 0 and 255")])?;
    let infix = infix_precedence
        .map(|precedence| {
            let associativity = match optional_identifier_or_enum(node, "associativity")? {
                Some("left") => ExpressionAssociativity::Left,
                Some("right") => ExpressionAssociativity::Right,
                Some(other) => {
                    return Err(vec![error(
                        node,
                        format!("unsupported expression associativity `{other}`"),
                    )]);
                }
                None => return Err(vec![error(node, "infix operator requires associativity")]),
            };
            Ok(InfixOperatorSpec {
                precedence,
                associativity,
            })
        })
        .transpose()?;
    Ok(ExpressionOperatorSpec {
        token,
        prefix_precedence,
        infix,
    })
}

fn unresolved_declared_type<'a>(
    diagnostics: &'a [Diagnostic],
    declared_type_ids: &BTreeMap<String, &str>,
    enums: &BTreeMap<String, Vec<String>>,
    named_types: &BTreeMap<String, Vec<FieldSpec>>,
    properties: &BTreeMap<String, Vec<FieldSpec>>,
) -> Option<&'a str> {
    const UNRESOLVED_PREFIXES: [&str; 3] = [
        "unknown field type `",
        "unknown or forward property type `",
        "unknown or forward property bundle `",
    ];
    diagnostics.iter().find_map(|diagnostic| {
        let name = UNRESOLVED_PREFIXES
            .iter()
            .find_map(|prefix| diagnostic.message.strip_prefix(prefix))?
            .split_once('`')?
            .0;
        (declared_type_ids.contains_key(name)
            && !enums.contains_key(name)
            && !named_types.contains_key(name)
            && !properties.contains_key(name))
        .then_some(name)
    })
}

fn schema_version(node: &AstNode, source: &SourceFile) -> Result<u32, Vec<Diagnostic>> {
    let Some(declaration) = optional_child(node, "schema_version_decl")? else {
        return Ok(1);
    };
    let value = optional_integer(declaration, "value", source)?.ok_or_else(|| {
        vec![error(
            declaration,
            "schema_version declaration is missing its integer value",
        )]
    })?;
    u32::try_from(value).map_err(|_| {
        vec![error(
            declaration,
            "schema_version must be a non-negative 32-bit integer",
        )]
    })
}

fn convert_rule(node: &AstNode) -> Result<(String, Vec<String>, String), Vec<Diagnostic>> {
    let parameters = match node.fields.get("parameters").map(|field| &field.value) {
        Some(Value::List(parameters)) => parameters
            .iter()
            .map(|parameter| match &parameter.value {
                Value::Identifier(parameter) => Ok(parameter.clone()),
                _ => Err(vec![error(
                    node,
                    "syntax rule parameters must be identifiers",
                )]),
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err(vec![error(node, "syntax rule parameters must be a list")]),
        None => Vec::new(),
    };
    Ok((identifier(node, "id")?, parameters, text(node, "pattern")?))
}

fn convert_enum(node: &AstNode) -> Result<(String, Vec<String>), Vec<Diagnostic>> {
    let id = identifier(node, "id")?;
    let mut values = Vec::new();
    for child in &node.children {
        if child.construct != "enum_value" {
            return Err(vec![error(
                child,
                format!("unsupported child `{}` in enum `{id}`", child.construct),
            )]);
        }
        let value = identifier(child, "value")?;
        if values.contains(&value) {
            return Err(vec![Diagnostic::error(
                "LMDEF027",
                format!("enum `{id}` repeats value `{value}`"),
                child.span,
            )]);
        }
        values.push(value);
    }
    if values.is_empty() {
        return Err(vec![error(
            node,
            "enum declarations must contain at least one value",
        )]);
    }
    Ok((id, values))
}

fn convert_behavior(node: &AstNode) -> Result<(String, BehaviorStage), Vec<Diagnostic>> {
    let id = identifier(node, "id")?;
    let stage_name = identifier(node, "stage")?;
    let stage = match stage_name.as_str() {
        "semantic_analysis" => BehaviorStage::SemanticAnalysis,
        "symbol_resolution" => BehaviorStage::SymbolResolution,
        "workspace_type_analysis" => BehaviorStage::WorkspaceTypeAnalysis,
        other => {
            return Err(vec![error(
                node,
                format!("unsupported behavior stage `{other}`"),
            )]);
        }
    };
    Ok((id, stage))
}

fn convert_type(
    node: &AstNode,
    enums: &BTreeMap<String, Vec<String>>,
    named_types: &BTreeMap<String, Vec<FieldSpec>>,
    properties: &BTreeMap<String, Vec<FieldSpec>>,
    source: &SourceFile,
) -> Result<(String, Vec<FieldSpec>), Vec<Diagnostic>> {
    let id = identifier(node, "id")?;
    let fields = convert_field_declarations(node, enums, named_types, properties, source)?;
    if fields.is_empty() {
        return Err(vec![error(
            node,
            "record types must declare at least one field",
        )]);
    }
    Ok((id, fields))
}

fn convert_properties(
    node: &AstNode,
    enums: &BTreeMap<String, Vec<String>>,
    named_types: &BTreeMap<String, Vec<FieldSpec>>,
    properties: &BTreeMap<String, Vec<FieldSpec>>,
    source: &SourceFile,
) -> Result<(String, Vec<FieldSpec>), Vec<Diagnostic>> {
    let id = identifier(node, "id")?;
    let fields = convert_field_declarations(node, enums, named_types, properties, source)?;
    if fields.is_empty() {
        return Err(vec![error(
            node,
            "properties declarations must contain at least one field",
        )]);
    }
    Ok((id, fields))
}
