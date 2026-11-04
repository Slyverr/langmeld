//! Canonical language schema validation and declarative syntax conflict analysis.

use std::collections::{BTreeMap, BTreeSet};

use super::*;

mod patterns;
use patterns::*;

impl LanguageSpec {
    /// Validates references and the syntax-template subset supported by the shared parser.
    ///
    /// # Errors
    /// Returns every detected definition error. No parser or semantic service accepts an
    /// unvalidated definition.
    pub fn validate(mut self) -> Result<ValidatedLanguageSpec, Vec<SpecError>> {
        let mut errors = Vec::new();
        let mut behavior_ids = BTreeSet::new();
        for behavior in &self.behaviors {
            if !valid_identifier(&behavior.id) || !behavior_ids.insert(behavior.id.as_str()) {
                errors.push(SpecError {
                    code: "LM1059",
                    message: format!(
                        "behavior id `{}` is invalid or declared more than once",
                        behavior.id
                    ),
                });
            }
        }
        let mut rules = BTreeMap::new();
        for rule in &self.rules {
            if !valid_identifier(&rule.id) {
                errors.push(SpecError {
                    code: "LM1051",
                    message: format!("syntax rule id `{}` must be an identifier", rule.id),
                });
            }
            if rule.pattern.trim().is_empty() {
                errors.push(SpecError {
                    code: "LM1051",
                    message: format!("syntax rule `{}` must not be empty", rule.id),
                });
            }
            let mut parameters = BTreeSet::new();
            for parameter in &rule.parameters {
                if !valid_identifier(parameter) || !parameters.insert(parameter.as_str()) {
                    errors.push(SpecError {
                        code: "LM1052",
                        message: format!(
                            "syntax rule `{}` has an invalid or duplicate parameter `{parameter}`",
                            rule.id
                        ),
                    });
                }
            }
            if rules.insert(rule.id.clone(), rule.clone()).is_some() {
                errors.push(SpecError {
                    code: "LM1051",
                    message: format!("duplicate syntax rule id `{}`", rule.id),
                });
            }
        }
        for rule in &mut self.rules {
            match expand_syntax_fragment(&rule.pattern, &rules, &mut vec![rule.id.clone()]) {
                Ok(expanded) => {
                    rule.pattern = expanded;
                    if let Err(message) = pattern_placeholders(&rule.pattern) {
                        errors.push(SpecError {
                            code: "LM1052",
                            message: format!("syntax rule `{}`: {message}", rule.id),
                        });
                    }
                }
                Err(message) => errors.push(SpecError {
                    code: "LM1051",
                    message: format!("syntax rule `{}`: {message}", rule.id),
                }),
            }
        }
        for construct in &mut self.constructs {
            for pattern in &mut construct.syntax {
                match expand_syntax_fragment(pattern, &rules, &mut Vec::new()) {
                    Ok(expanded) => *pattern = expanded,
                    Err(message) => errors.push(SpecError {
                        code: "LM1051",
                        message: format!("construct `{}`: {message}", construct.id),
                    }),
                }
            }
        }
        let mut construct_ids = BTreeSet::new();
        let constructs: BTreeMap<_, _> = self
            .constructs
            .iter()
            .map(|construct| (construct.id.as_str(), construct))
            .collect();
        let categories: BTreeMap<_, _> = self
            .categories
            .iter()
            .map(|category| (category.id.as_str(), category))
            .collect();
        let mut category_ids = BTreeSet::new();
        for category in &self.categories {
            if !valid_identifier(&category.id) {
                errors.push(SpecError {
                    code: "LM1038",
                    message: format!("category id `{}` must be an identifier", category.id),
                });
            }
            if !category_ids.insert(category.id.as_str()) {
                errors.push(SpecError {
                    code: "LM1039",
                    message: format!("duplicate category id `{}`", category.id),
                });
            }
            if constructs.contains_key(category.id.as_str()) {
                errors.push(SpecError {
                    code: "LM1040",
                    message: format!(
                        "category id `{}` conflicts with a construct id",
                        category.id
                    ),
                });
            }
        }

        let mut type_ids = BTreeSet::new();
        for enumeration in &self.enums {
            if !valid_identifier(&enumeration.id) {
                errors.push(SpecError {
                    code: "LM1063",
                    message: format!("enum id `{}` must be an identifier", enumeration.id),
                });
            }
            if !type_ids.insert(enumeration.id.as_str()) {
                errors.push(SpecError {
                    code: "LM1064",
                    message: format!("duplicate type or enum id `{}`", enumeration.id),
                });
            }
            if enumeration.values.is_empty() {
                errors.push(SpecError {
                    code: "LM1065",
                    message: format!("enum `{}` must declare at least one value", enumeration.id),
                });
            }
            let mut values = BTreeSet::new();
            for value in &enumeration.values {
                if !valid_identifier(value) || !values.insert(value.as_str()) {
                    errors.push(SpecError {
                        code: "LM1066",
                        message: format!(
                            "enum `{}` has an invalid or duplicate value `{value}`",
                            enumeration.id
                        ),
                    });
                }
            }
            if constructs.contains_key(enumeration.id.as_str())
                || category_ids.contains(enumeration.id.as_str())
            {
                errors.push(SpecError {
                    code: "LM1067",
                    message: format!(
                        "enum id `{}` conflicts with a construct or category id",
                        enumeration.id
                    ),
                });
            }
        }
        for named_type in &self.types {
            if !valid_identifier(&named_type.id) {
                errors.push(SpecError {
                    code: "LM1053",
                    message: format!("type id `{}` must be an identifier", named_type.id),
                });
            }
            if !type_ids.insert(named_type.id.as_str()) {
                errors.push(SpecError {
                    code: "LM1054",
                    message: format!("duplicate type id `{}`", named_type.id),
                });
            }
            if named_type.fields.is_empty() {
                errors.push(SpecError {
                    code: "LM1055",
                    message: format!("type `{}` must declare at least one field", named_type.id),
                });
            }
            if constructs.contains_key(named_type.id.as_str())
                || category_ids.contains(named_type.id.as_str())
            {
                errors.push(SpecError {
                    code: "LM1056",
                    message: format!(
                        "type id `{}` conflicts with a construct or category id",
                        named_type.id
                    ),
                });
            }
            validate_property_fields(&named_type.fields, &named_type.id, &mut errors);
        }
        let named_types: BTreeMap<_, _> = self
            .types
            .iter()
            .map(|named_type| (named_type.id.as_str(), named_type.fields.as_slice()))
            .collect();
        let named_enums: BTreeMap<_, _> = self
            .enums
            .iter()
            .map(|enumeration| (enumeration.id.as_str(), enumeration.values.as_slice()))
            .collect();
        for named_type in &self.types {
            for field in &named_type.fields {
                validate_named_type_usage(
                    &field.kind,
                    &named_types,
                    &named_enums,
                    &format!("{}.{}", named_type.id, field.name),
                    &mut errors,
                );
            }
        }
        for construct in &self.constructs {
            for field in &construct.fields {
                validate_named_type_usage(
                    &field.kind,
                    &named_types,
                    &named_enums,
                    &format!("{}.{}", construct.id, field.name),
                    &mut errors,
                );
            }
        }

        if !matches!(self.schema_version, 1 | 2) {
            errors.push(SpecError {
                code: "LM1001",
                message: format!(
                    "unsupported schema_version {}; expected 1 or 2",
                    self.schema_version
                ),
            });
        }
        if self.name.trim().is_empty() || self.version.trim().is_empty() {
            errors.push(SpecError {
                code: "LM1002",
                message: "language name and version must not be empty".to_owned(),
            });
        }

        let mut source_extensions = BTreeSet::new();
        for extension in &mut self.source_extensions {
            let raw_extension = extension.trim();
            let canonical = if raw_extension.starts_with('.') {
                raw_extension.to_owned()
            } else {
                format!(".{raw_extension}")
            };
            let suffix = canonical.strip_prefix('.').unwrap_or_default();
            if suffix.is_empty()
                || suffix.starts_with('.')
                || suffix.ends_with('.')
                || suffix.contains("..")
                || canonical.chars().any(char::is_whitespace)
                || canonical.contains('/')
                || canonical.contains('\\')
            {
                errors.push(SpecError {
                    code: "LM1061",
                    message: format!("source extension `{raw_extension}` is invalid"),
                });
                continue;
            }
            if !source_extensions.insert(canonical.clone()) {
                errors.push(SpecError {
                    code: "LM1062",
                    message: format!("duplicate source extension `{canonical}`"),
                });
            }
            *extension = canonical;
        }

        for construct in &self.constructs {
            if !construct_ids.insert(construct.id.as_str()) {
                errors.push(SpecError {
                    code: "LM1003",
                    message: format!("duplicate construct id `{}`", construct.id),
                });
            }
            if construct.syntax.is_empty()
                && construct.custom_parser.is_none()
                && !is_custom_parser_owned_child(construct, &self.constructs, &categories)
            {
                errors.push(SpecError {
                    code: "LM1004",
                    message: format!("construct `{}` has no syntax patterns", construct.id),
                });
            }
            if let Some(parser) = &construct.custom_parser
                && !valid_identifier(parser)
            {
                errors.push(SpecError {
                    code: "LM1032",
                    message: format!(
                        "custom parser ID `{parser}` for `{}` must be an identifier",
                        construct.id
                    ),
                });
            }
            let mut implemented = BTreeSet::new();
            for category in &construct.implements {
                if !implemented.insert(category.as_str()) {
                    errors.push(SpecError {
                        code: "LM1041",
                        message: format!(
                            "construct `{}` implements category `{category}` more than once",
                            construct.id
                        ),
                    });
                }
                if !categories.contains_key(category.as_str()) {
                    errors.push(SpecError {
                        code: "LM1042",
                        message: format!(
                            "construct `{}` implements unknown category `{category}`",
                            construct.id
                        ),
                    });
                }
            }
            if !construct.syntax_ids.is_empty()
                && construct.syntax_ids.len() != construct.syntax.len()
            {
                errors.push(SpecError {
                    code: "LM1021",
                    message: format!(
                        "construct `{}` must provide one syntax id per pattern",
                        construct.id
                    ),
                });
            }
            let mut syntax_ids = BTreeSet::new();
            for syntax_id in &construct.syntax_ids {
                if !valid_identifier(syntax_id) {
                    errors.push(SpecError {
                        code: "LM1022",
                        message: format!(
                            "syntax id `{syntax_id}` in `{}` must be an identifier",
                            construct.id
                        ),
                    });
                }
                if !syntax_ids.insert(syntax_id.as_str()) {
                    errors.push(SpecError {
                        code: "LM1023",
                        message: format!(
                            "construct `{}` repeats syntax id `{syntax_id}`",
                            construct.id
                        ),
                    });
                }
            }

            let fields: BTreeMap<_, _> = construct
                .fields
                .iter()
                .map(|field| (field.name.as_str(), field))
                .collect();
            if fields.len() != construct.fields.len() {
                errors.push(SpecError {
                    code: "LM1005",
                    message: format!("construct `{}` has duplicate field names", construct.id),
                });
            }
            for field in &construct.fields {
                validate_field_constraints(
                    &field.constraints,
                    &field.kind,
                    &format!("{}.{}", construct.id, field.name),
                    &mut errors,
                );
                if let Some(default) = &field.default {
                    let valid_default = match &field.kind {
                        FieldKind::Identifier => valid_identifier(default),
                        FieldKind::Number => default.parse::<f64>().is_ok_and(f64::is_finite),
                        FieldKind::Text => true,
                        FieldKind::Enum { values } | FieldKind::NamedEnum { values, .. } => {
                            values.contains(default)
                        }
                        FieldKind::Reference { .. }
                        | FieldKind::Properties { .. }
                        | FieldKind::List { .. }
                        | FieldKind::Map { .. } => false,
                    };
                    if !valid_default {
                        errors.push(SpecError {
                            code: "LM1025",
                            message: format!(
                                "default `{default}` is invalid for field `{}.{}`",
                                construct.id, field.name
                            ),
                        });
                    } else if !default_satisfies_constraints(
                        default,
                        &field.kind,
                        &field.constraints,
                    ) {
                        errors.push(SpecError {
                            code: "LM1046",
                            message: format!(
                                "default `{default}` violates constraints for field `{}.{}`",
                                construct.id, field.name
                            ),
                        });
                    }
                }
                if let FieldKind::Enum { values } = &field.kind {
                    if values.is_empty() {
                        errors.push(SpecError {
                            code: "LM1018",
                            message: format!(
                                "enum field `{}.{}` has no values",
                                construct.id, field.name
                            ),
                        });
                    }
                    let mut enum_values = BTreeSet::new();
                    for value in values {
                        if !enum_values.insert(value.as_str()) {
                            errors.push(SpecError {
                                code: "LM1019",
                                message: format!(
                                    "enum field `{}.{}` repeats value `{value}`",
                                    construct.id, field.name
                                ),
                            });
                        }
                        if !valid_identifier(value) {
                            errors.push(SpecError {
                                code: "LM1020",
                                message: format!(
                                    "enum value `{value}` in `{}.{}` must be an identifier",
                                    construct.id, field.name
                                ),
                            });
                        }
                    }
                }
                if let FieldKind::Properties {
                    fields: properties, ..
                } = &field.kind
                {
                    validate_property_fields(
                        properties,
                        &format!("{}.{}", construct.id, field.name),
                        &mut errors,
                    );
                }
                if let FieldKind::List { item } = &field.kind {
                    validate_list_item(
                        item,
                        &format!("{}.{}", construct.id, field.name),
                        &mut errors,
                    );
                }
                if let FieldKind::Map { key, value } = &field.kind {
                    validate_map(
                        key,
                        value,
                        &format!("{}.{}", construct.id, field.name),
                        &mut errors,
                    );
                }
            }

            if let Some(block) = &construct.block {
                if self.schema_version == 1 && !block.uses_default_layout() {
                    errors.push(SpecError {
                        code: "LM1071",
                        message: format!(
                            "construct `{}` uses non-default block settings; set schema_version to 2",
                            construct.id
                        ),
                    });
                }
                if self.schema_version == 2 {
                    if block.open.trim().is_empty() || block.close.trim().is_empty() {
                        errors.push(SpecError {
                            code: "LM1072",
                            message: format!(
                                "block `{}` opener and closer must not be empty",
                                construct.id
                            ),
                        });
                    }
                    if block.open == block.close {
                        errors.push(SpecError {
                            code: "LM1073",
                            message: format!(
                                "block `{}` opener and closer must differ",
                                construct.id
                            ),
                        });
                    }
                    if block.separator_policy == crate::BlockSeparatorPolicy::Required
                        && block.separators.is_empty()
                    {
                        errors.push(SpecError {
                            code: "LM1074",
                            message: format!(
                                "block `{}` requires at least one separator declaration",
                                construct.id
                            ),
                        });
                    }
                    let mut separators = Vec::new();
                    for separator in &block.separators {
                        if let crate::BlockSeparator::Literal(value) = separator {
                            if value.trim().is_empty() {
                                errors.push(SpecError {
                                    code: "LM1075",
                                    message: format!(
                                        "block `{}` has an empty literal separator",
                                        construct.id
                                    ),
                                });
                            }
                            if value == &block.open || value == &block.close {
                                errors.push(SpecError {
                                    code: "LM1076",
                                    message: format!(
                                        "block `{}` separator `{value}` conflicts with a delimiter",
                                        construct.id
                                    ),
                                });
                            }
                        }
                        if separators.contains(separator) {
                            errors.push(SpecError {
                                code: "LM1077",
                                message: format!(
                                    "block `{}` repeats a separator declaration",
                                    construct.id
                                ),
                            });
                        } else {
                            separators.push(separator.clone());
                        }
                    }
                }
            }

            let mut patterns = BTreeSet::new();
            for pattern in &construct.syntax {
                if !patterns.insert(pattern) {
                    errors.push(SpecError {
                        code: "LM1006",
                        message: format!(
                            "construct `{}` repeats syntax pattern `{pattern}`",
                            construct.id
                        ),
                    });
                }
                match pattern_placeholders(pattern) {
                    Ok(placeholders) => {
                        let mut seen = BTreeSet::new();
                        let mut unconditional = BTreeSet::new();
                        let mut record_captures: BTreeMap<String, BTreeSet<String>> =
                            BTreeMap::new();
                        for (placeholder, optional, repeated) in placeholders {
                            if !seen.insert(placeholder.clone()) {
                                errors.push(SpecError {
                                    code: "LM1007",
                                    message: format!("pattern `{pattern}` captures `{placeholder}` more than once"),
                                });
                            }
                            let (root, property) = placeholder
                                .split_once('.')
                                .map_or((placeholder.as_str(), None), |(root, property)| {
                                    (root, Some(property))
                                });
                            let Some(field) = fields.get(root) else {
                                errors.push(SpecError {
                                    code: "LM1008",
                                    message: format!("pattern `{pattern}` refers to undeclared field `{placeholder}`"),
                                });
                                continue;
                            };
                            if repeated > 0
                                && pattern_capture_kind(
                                    construct,
                                    &(placeholder.clone(), optional, repeated),
                                )
                                .is_none()
                            {
                                errors.push(SpecError {
                                    code: "LM1050",
                                    message: format!(
                                        "capture `{placeholder}` repeated {repeated} times in pattern `{pattern}` requires {repeated} nested list fields"
                                    ),
                                });
                            }
                            if let Some(property) = property {
                                if repeated == 0 {
                                    errors.push(SpecError {
                                        code: "LM1051",
                                        message: format!("nested capture `{placeholder}` must be inside a repetition group"),
                                    });
                                }
                                if repeated != 1 {
                                    errors.push(SpecError {
                                        code: "LM1051",
                                        message: format!("nested property capture `{placeholder}` currently requires exactly one repetition group"),
                                    });
                                    continue;
                                }
                                let FieldKind::List { item } = &field.kind else {
                                    errors.push(SpecError {
                                        code: "LM1051",
                                        message: format!("nested capture `{placeholder}` requires a list of properties"),
                                    });
                                    continue;
                                };
                                let FieldKind::Properties {
                                    fields: properties, ..
                                } = item.as_ref()
                                else {
                                    errors.push(SpecError {
                                        code: "LM1051",
                                        message: format!("nested capture `{placeholder}` requires a list of properties"),
                                    });
                                    continue;
                                };
                                if !properties
                                    .iter()
                                    .any(|candidate| candidate.name == property)
                                {
                                    errors.push(SpecError {
                                        code: "LM1052",
                                        message: format!("pattern `{pattern}` refers to undeclared property `{placeholder}`"),
                                    });
                                }
                                record_captures
                                    .entry(root.to_owned())
                                    .or_default()
                                    .insert(property.to_owned());
                            }
                            if !optional {
                                unconditional.insert(root.to_owned());
                            }
                        }
                        for (root, captured) in record_captures {
                            if let Some(FieldKind::List { item }) =
                                fields.get(root.as_str()).map(|field| &field.kind)
                                && let FieldKind::Properties {
                                    fields: properties, ..
                                } = item.as_ref()
                            {
                                for property in properties.iter().filter(|property| {
                                    property.required && property.default.is_none()
                                }) {
                                    if !captured.contains(&property.name) {
                                        errors.push(SpecError {
                                            code: "LM1053",
                                            message: format!("pattern `{pattern}` omits required property `{}.{}`", root, property.name),
                                        });
                                    }
                                }
                            }
                        }
                        for field in &construct.fields {
                            if field.required
                                && field.default.is_none()
                                && !unconditional.contains(&field.name)
                            {
                                errors.push(SpecError {
                                    code: "LM1009",
                                    message: format!(
                                        "pattern `{pattern}` omits required field `{}`",
                                        field.name
                                    ),
                                });
                            }
                        }
                    }
                    Err(message) => errors.push(SpecError {
                        code: "LM1010",
                        message,
                    }),
                }
                let custom_parser_owns_children = construct.custom_parser.is_some()
                    || is_custom_parser_owned_child(construct, &self.constructs, &categories);
                if let Some(block) = &construct.block {
                    let inline_opener = match self.schema_version {
                        1 => !pattern.trim_end().ends_with('{'),
                        2 => pattern_ends_with_literal(pattern, &block.open),
                        _ => false,
                    };
                    if inline_opener && !custom_parser_owns_children {
                        let message = if self.schema_version == 1 {
                            format!(
                                "block construct `{}` must end every pattern with `{{`",
                                construct.id
                            )
                        } else {
                            format!(
                                "block construct `{}` owns its opener; remove `{}` from the syntax pattern",
                                construct.id, block.open
                            )
                        };
                        errors.push(SpecError {
                            code: "LM1011",
                            message,
                        });
                    }
                }
            }

            if let Some(symbol) = &construct.symbol {
                match fields.get(symbol.name_field.as_str()) {
                    Some(field) if field.default.is_some() => errors.push(SpecError {
                        code: "LM1026",
                        message: format!(
                            "symbol name field `{}.{}` cannot have a default",
                            construct.id, symbol.name_field
                        ),
                    }),
                    Some(field) if matches!(field.kind, FieldKind::Identifier) => {}
                    Some(_) => errors.push(SpecError {
                        code: "LM1012",
                        message: format!(
                            "symbol name field `{}.{}` must be an identifier",
                            construct.id, symbol.name_field
                        ),
                    }),
                    None => errors.push(SpecError {
                        code: "LM1013",
                        message: format!(
                            "symbol `{}` refers to missing name field `{}`",
                            construct.id, symbol.name_field
                        ),
                    }),
                }
            }
            if let Some(symbol) = &construct.symbol
                && let Some(scope) = &symbol.scope
            {
                if let Some(scope_construct) = constructs.get(scope.construct.as_str()) {
                    match scope_construct
                        .fields
                        .iter()
                        .find(|field| field.name == scope.name_field)
                    {
                        Some(field)
                            if matches!(field.kind, FieldKind::Identifier)
                                && field.required
                                && field.default.is_none() => {}
                        Some(_) => errors.push(SpecError {
                            code: "LM1028",
                            message: format!(
                                "scope name field `{}.{}` must be a required identifier without a default",
                                scope.construct, scope.name_field
                            ),
                        }),
                        None => errors.push(SpecError {
                            code: "LM1029",
                            message: format!(
                                "symbol `{}` refers to missing scope name field `{}.{}`",
                                construct.id, scope.construct, scope.name_field
                            ),
                        }),
                    }
                    if scope.construct != construct.id
                        && !can_contain_construct(&constructs, &scope.construct, &construct.id)
                    {
                        errors.push(SpecError {
                            code: "LM1030",
                            message: format!(
                                "scope construct `{}` cannot contain symbol construct `{}`",
                                scope.construct, construct.id
                            ),
                        });
                    }
                } else {
                    errors.push(SpecError {
                        code: "LM1027",
                        message: format!(
                            "symbol `{}` refers to unknown scope construct `{}`",
                            construct.id, scope.construct
                        ),
                    });
                }
            }
        }

        let mut symbol_scopes = BTreeMap::new();
        for construct in &self.constructs {
            if let Some(symbol) = &construct.symbol
                && let Some(previous) = symbol_scopes.insert(&symbol.kind, &symbol.scope)
                && previous != &symbol.scope
            {
                errors.push(SpecError {
                    code: "LM1031",
                    message: format!(
                        "symbol kind `{}` has conflicting scope policies",
                        symbol.kind
                    ),
                });
            }
        }

        if !construct_ids.contains(self.root.as_str()) {
            errors.push(SpecError {
                code: "LM1014",
                message: format!("root construct `{}` is not declared", self.root),
            });
        } else if root_requires_block(&self.constructs, &self.root) {
            errors.push(SpecError {
                code: "LM1017",
                message: format!("root construct `{}` must declare a block", self.root),
            });
        }

        for construct in &self.constructs {
            if let Some(block) = &construct.block {
                let mut expanded_children = BTreeSet::new();
                for child in &block.allowed {
                    if !constructs.contains_key(child.as_str())
                        && !categories.contains_key(child.as_str())
                    {
                        errors.push(SpecError {
                            code: "LM1015",
                            message: format!(
                                "block `{}` allows unknown construct `{child}`",
                                construct.id
                            ),
                        });
                    }
                    for child_id in if constructs.contains_key(child.as_str()) {
                        vec![child.as_str()]
                    } else {
                        self.constructs
                            .iter()
                            .filter(|candidate| candidate.implements.contains(child))
                            .map(|candidate| candidate.id.as_str())
                            .collect()
                    } {
                        if !expanded_children.insert(child_id) {
                            errors.push(SpecError {
                                code: "LM1043",
                                message: format!(
                                    "block `{}` allows construct `{child_id}` more than once through direct or category entries",
                                    construct.id
                                ),
                            });
                        }
                    }
                }

                let mut child_patterns: BTreeMap<String, Vec<PatternConflictCandidate<'_>>> =
                    BTreeMap::new();
                let mut token_patterns: BTreeMap<usize, Vec<PatternConflictCandidate<'_>>> =
                    BTreeMap::new();
                let mut token_pattern_comparisons = 0;
                let mut same_shape_comparisons = 0;
                let mut candidate_count = 0;
                let mut automata: Vec<PatternAutomatonCandidate<'_>> = Vec::new();
                let mut automata_pairs = 0;
                let mut automata_work = 0;
                let mut automata_analyzed_pairs = BTreeSet::new();
                let mut automata_conflicts = BTreeSet::new();
                'children: for child_id in &expanded_children {
                    let Some(child) = constructs.get(*child_id) else {
                        continue;
                    };
                    let effective_patterns = child
                        .syntax
                        .iter()
                        .map(|pattern| {
                            if self.schema_version == 2
                                && let Some(block) = &child.block
                            {
                                return format!(
                                    "{pattern} {}",
                                    escape_pattern_literal(&block.open)
                                );
                            }
                            pattern.clone()
                        })
                        .collect::<Vec<_>>();
                    for pattern in &effective_patterns {
                        if candidate_count >= MAX_PATTERN_REALIZATIONS_PER_BLOCK {
                            break 'children;
                        }
                        let automaton = pattern_token_automaton(pattern, child);
                        let automaton_supported = automaton.is_some();
                        let has_repetition = automaton
                            .as_ref()
                            .is_some_and(|automaton| automaton.has_repetition);
                        if let Some(automaton) = automaton {
                            for previous in &automata {
                                if previous.construct.id == child.id && previous.pattern == *pattern
                                {
                                    continue;
                                }
                                if !automaton.has_repetition && !previous.automaton.has_repetition {
                                    continue;
                                }
                                if automata_pairs >= MAX_PATTERN_CONFLICT_COMPARISONS
                                    || automata_work >= MAX_PATTERN_AUTOMATON_WORK_PER_BLOCK
                                {
                                    break;
                                }
                                let key = pattern_conflict_key(
                                    previous.construct,
                                    &previous.pattern,
                                    child,
                                    pattern,
                                );
                                let (overlaps, work) = pattern_automata_overlap(
                                    previous.construct,
                                    &previous.automaton,
                                    child,
                                    &automaton,
                                    MAX_PATTERN_AUTOMATON_WORK_PER_BLOCK - automata_work,
                                );
                                automata_pairs += 1;
                                automata_work = automata_work
                                    .saturating_add(work)
                                    .min(MAX_PATTERN_AUTOMATON_WORK_PER_BLOCK);
                                if let Some(overlaps) = overlaps {
                                    automata_analyzed_pairs.insert(key.clone());
                                    if overlaps && automata_conflicts.insert(key) {
                                        errors.push(SpecError {
                                            code: "LM1060",
                                            message: format!(
                                                "block `{}` allows `{}` pattern `{}` and `{}` pattern `{}` that can both match the same token sequence",
                                                construct.id,
                                                previous.construct.id,
                                                previous.pattern,
                                                child.id,
                                                pattern
                                            ),
                                        });
                                    }
                                } else {
                                    automata_work = MAX_PATTERN_AUTOMATON_WORK_PER_BLOCK;
                                    break;
                                }
                            }
                            automata.push(PatternAutomatonCandidate {
                                construct: child,
                                pattern: pattern.clone(),
                                automaton,
                            });
                        }
                        let repeated_fields = pattern_placeholders(pattern)
                            .unwrap_or_default()
                            .into_iter()
                            .fold(
                                BTreeMap::<String, usize>::new(),
                                |mut fields, (field, _, depth)| {
                                    fields
                                        .entry(field)
                                        .and_modify(|existing| *existing = (*existing).max(depth))
                                        .or_insert(depth);
                                    fields
                                },
                            );
                        let realizations = pattern_realizations(pattern)
                            .unwrap_or_else(|| vec![normalize_pattern_whitespace(pattern)]);
                        for realization in realizations {
                            if candidate_count >= MAX_PATTERN_REALIZATIONS_PER_BLOCK {
                                break 'children;
                            }
                            candidate_count += 1;
                            let Ok(captures) = pattern_placeholders(&realization) else {
                                continue;
                            };
                            let captures: Vec<_> = captures
                                .into_iter()
                                .map(|(field, optional, repeated)| {
                                    let repeated = repeated.max(
                                        repeated_fields.get(&field).copied().unwrap_or_default(),
                                    );
                                    (field, optional, repeated)
                                })
                                .collect();
                            let signature = normalize_pattern_captures(&realization);
                            let mut atoms = pattern_atoms(&realization);
                            if let Some(atoms) = &mut atoms {
                                for atom in atoms {
                                    if let PatternAtom::Capture(field, repeated) = atom {
                                        *repeated =
                                            repeated_fields.get(field).copied().unwrap_or_default();
                                    }
                                }
                            }
                            let candidate = PatternConflictCandidate {
                                construct: child,
                                pattern: pattern.clone(),
                                captures,
                                signature: signature.clone(),
                                atoms,
                                has_repetition,
                                automaton_supported,
                            };
                            let previous = child_patterns.entry(signature).or_default();
                            for previous_candidate in previous.iter() {
                                let pair_key = pattern_conflict_key(
                                    previous_candidate.construct,
                                    &previous_candidate.pattern,
                                    child,
                                    pattern,
                                );
                                if previous_candidate.automaton_supported
                                    && candidate.automaton_supported
                                    && (previous_candidate.has_repetition
                                        || candidate.has_repetition)
                                    && automata_analyzed_pairs.contains(&pair_key)
                                {
                                    continue;
                                }
                                if same_shape_comparisons >= MAX_PATTERN_CONFLICT_COMPARISONS {
                                    break;
                                }
                                same_shape_comparisons += 1;
                                if previous_candidate.construct.id == child.id
                                    && previous_candidate.pattern == *pattern
                                {
                                    continue;
                                }
                                if pattern_captures_can_overlap(
                                    previous_candidate.construct,
                                    &previous_candidate.captures,
                                    child,
                                    &candidate.captures,
                                ) {
                                    errors.push(SpecError {
                                        code: "LM1060",
                                        message: format!(
                                            "block `{}` allows `{}` pattern `{previous_pattern}` and `{}` pattern `{pattern}` that can both match `{realization}`",
                                            construct.id,
                                            previous_candidate.construct.id,
                                            child.id,
                                            previous_pattern = previous_candidate.pattern
                                        ),
                                    });
                                    break;
                                }
                            }
                            previous.push(candidate.clone());

                            if token_pattern_comparisons < MAX_PATTERN_CONFLICT_COMPARISONS
                                && let Some(atoms) = &candidate.atoms
                            {
                                let same_length = token_patterns.entry(atoms.len()).or_default();
                                for previous_candidate in same_length.iter() {
                                    let pair_key = pattern_conflict_key(
                                        previous_candidate.construct,
                                        &previous_candidate.pattern,
                                        child,
                                        pattern,
                                    );
                                    if previous_candidate.automaton_supported
                                        && candidate.automaton_supported
                                        && (previous_candidate.has_repetition
                                            || candidate.has_repetition)
                                        && automata_analyzed_pairs.contains(&pair_key)
                                    {
                                        continue;
                                    }
                                    if token_pattern_comparisons >= MAX_PATTERN_CONFLICT_COMPARISONS
                                    {
                                        break;
                                    }
                                    token_pattern_comparisons += 1;
                                    if previous_candidate.signature == candidate.signature
                                        || (previous_candidate.construct.id == child.id
                                            && previous_candidate.pattern == *pattern)
                                    {
                                        continue;
                                    }
                                    let Some(previous_atoms) = &previous_candidate.atoms else {
                                        continue;
                                    };
                                    if pattern_atoms_can_overlap(
                                        previous_candidate.construct,
                                        previous_atoms,
                                        candidate.construct,
                                        atoms,
                                    ) {
                                        errors.push(SpecError {
                                            code: "LM1060",
                                            message: format!(
                                                "block `{}` allows `{}` pattern `{}` and `{}` pattern `{}` that can both match `{realization}`",
                                                construct.id,
                                                previous_candidate.construct.id,
                                                previous_candidate.pattern,
                                                candidate.construct.id,
                                                candidate.pattern
                                            ),
                                        });
                                        break;
                                    }
                                }
                                if same_length.len() < MAX_PATTERN_REALIZATIONS_PER_BLOCK {
                                    same_length.push(candidate);
                                }
                            }
                        }
                    }
                }
            }
            for field in &construct.fields {
                validate_reference_targets(
                    &field.kind,
                    &format!("{}:{}", construct.id, field.name),
                    &self.constructs,
                    &mut errors,
                );
            }
        }

        if errors.is_empty() {
            // Type declaration order has no parsing semantics, so normalize it in both
            // authoring frontends before exposing the validated canonical specification.
            self.enums.sort_by(|left, right| left.id.cmp(&right.id));
            self.types.sort_by(|left, right| left.id.cmp(&right.id));
            Ok(ValidatedLanguageSpec(self))
        } else {
            Err(errors)
        }
    }
}

fn is_custom_parser_owned_child(
    child: &ConstructSpec,
    constructs: &[ConstructSpec],
    categories: &BTreeMap<&str, &CategorySpec>,
) -> bool {
    let mut parents = constructs.iter().filter(|parent| {
        // A recursive block is still part of the custom parser's tree. Ignore
        // that self-edge when deciding whether an enclosing host parser owns it.
        parent.id != child.id
            && parent.block.as_ref().is_some_and(|block| {
                block.allowed.iter().any(|allowed| {
                    allowed == &child.id
                        || categories
                            .get(allowed.as_str())
                            .is_some_and(|category| child.implements.contains(&category.id))
                })
            })
    });
    let Some(first) = parents.next() else {
        return false;
    };
    first.custom_parser.is_some() && parents.all(|parent| parent.custom_parser.is_some())
}

fn pattern_ends_with_literal(pattern: &str, literal: &str) -> bool {
    let pattern = pattern
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let literal = literal
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let Some(prefix) = pattern.strip_suffix(&literal) else {
        return false;
    };
    let Some(first) = literal.chars().next() else {
        return false;
    };
    !identifier_character(first)
        || prefix
            .chars()
            .next_back()
            .is_none_or(|previous| !identifier_character(previous))
}

fn escape_pattern_literal(literal: &str) -> String {
    let mut output = String::with_capacity(literal.len());
    for character in literal.chars() {
        if matches!(
            character,
            '[' | ']' | '(' | ')' | '|' | '{' | '}' | '\\' | '@'
        ) {
            output.push('\\');
        }
        output.push(character);
    }
    output
}

fn identifier_character(character: char) -> bool {
    character == '_' || character.is_alphanumeric()
}

fn validate_reference_targets(
    kind: &FieldKind,
    path: &str,
    constructs: &[ConstructSpec],
    errors: &mut Vec<SpecError>,
) {
    match kind {
        FieldKind::Reference { target } => {
            if !constructs.iter().any(|candidate| {
                candidate
                    .symbol
                    .as_ref()
                    .is_some_and(|symbol| symbol.kind == *target)
            }) {
                errors.push(SpecError {
                    code: "LM1016",
                    message: format!(
                        "reference field `{path}` targets undeclared symbol kind `{target}`"
                    ),
                });
            }
        }
        FieldKind::Properties { fields, .. } => {
            for field in fields {
                validate_reference_targets(
                    &field.kind,
                    &format!("{path}.{}", field.name),
                    constructs,
                    errors,
                );
            }
        }
        FieldKind::List { item } => {
            validate_reference_targets(item, &format!("{path}[]"), constructs, errors);
        }
        FieldKind::Map { value, .. } => {
            validate_reference_targets(value, &format!("{path}{{}}"), constructs, errors);
        }
        FieldKind::Identifier
        | FieldKind::Number
        | FieldKind::Text
        | FieldKind::Enum { .. }
        | FieldKind::NamedEnum { .. } => {}
    }
}

fn expand_syntax_fragment(
    pattern: &str,
    rules: &BTreeMap<String, SyntaxRuleSpec>,
    stack: &mut Vec<String>,
) -> Result<String, String> {
    const MAX_RULE_DEPTH: usize = 32;
    const MAX_EXPANDED_BYTES: usize = 16 * 1024;

    let mut output = String::with_capacity(pattern.len());
    let mut characters = pattern.char_indices().peekable();
    while let Some((offset, character)) = characters.next() {
        if character == '\\' {
            output.push(character);
            let Some((_, escaped)) = characters.next() else {
                return Err("pattern ends with an incomplete escape".to_owned());
            };
            output.push(escaped);
        } else if character == '@' {
            let Some((_, first)) = characters.peek().copied() else {
                return Err(format!("expected a rule name after `@` at byte {offset}"));
            };
            if !(first == '_' || first.is_alphabetic()) {
                return Err(format!("expected a rule name after `@` at byte {offset}"));
            }
            let mut id = String::new();
            while let Some((_, next)) = characters.peek().copied() {
                if next == '_' || next == '-' || next.is_alphanumeric() {
                    id.push(next);
                    characters.next();
                } else {
                    break;
                }
            }
            if stack.len() >= MAX_RULE_DEPTH {
                return Err(format!(
                    "syntax rule expansion exceeds {MAX_RULE_DEPTH} levels"
                ));
            }
            if stack.contains(&id) {
                let mut cycle = stack.clone();
                cycle.push(id);
                return Err(format!("syntax rule cycle: {}", cycle.join(" -> ")));
            }
            let rule = rules
                .get(&id)
                .ok_or_else(|| format!("unknown syntax rule `@{id}`"))?;
            let mut raw_arguments = None;
            if matches!(characters.peek(), Some((_, '('))) {
                characters.next();
                let mut raw = String::new();
                let mut closed = false;
                for (_, next) in characters.by_ref() {
                    if next == ')' {
                        closed = true;
                        break;
                    }
                    raw.push(next);
                }
                if !closed {
                    return Err(format!(
                        "syntax rule `@{id}` has an unclosed parameter list"
                    ));
                }
                raw_arguments = Some(raw);
            }
            let raw_arguments = raw_arguments.as_deref().unwrap_or_default().trim();
            let arguments = if raw_arguments.is_empty() {
                Vec::new()
            } else {
                raw_arguments
                    .split(',')
                    .map(str::trim)
                    .map(str::to_owned)
                    .collect()
            };
            if arguments.iter().any(|argument| !valid_identifier(argument)) {
                return Err(format!(
                    "syntax rule `@{id}` arguments must be field identifiers"
                ));
            }
            if arguments.len() != rule.parameters.len() {
                return Err(format!(
                    "syntax rule `@{id}` expects {} arguments but received {}",
                    rule.parameters.len(),
                    arguments.len()
                ));
            }
            stack.push(id);
            let substituted =
                substitute_rule_parameters(&rule.pattern, &rule.parameters, &arguments)?;
            let expanded = expand_syntax_fragment(&substituted, rules, stack);
            stack.pop();
            output.push_str(&expanded?);
        } else {
            output.push(character);
        }
        if output.len() > MAX_EXPANDED_BYTES {
            return Err(format!(
                "expanded pattern exceeds {MAX_EXPANDED_BYTES} bytes"
            ));
        }
    }
    Ok(output)
}

fn substitute_rule_parameters(
    pattern: &str,
    parameters: &[String],
    arguments: &[String],
) -> Result<String, String> {
    const MAX_EXPANDED_BYTES: usize = 16 * 1024;
    let substitutions: BTreeMap<_, _> = parameters.iter().cloned().zip(arguments).collect();
    let mut output = String::with_capacity(pattern.len());
    let mut characters = pattern.char_indices().peekable();
    while let Some((_, character)) = characters.next() {
        if character == '\\' {
            output.push(character);
            let Some((_, escaped)) = characters.next() else {
                return Err("pattern ends with an incomplete escape".to_owned());
            };
            output.push(escaped);
        } else if character == '{' {
            let mut capture = String::new();
            let mut closed = false;
            for (_, next) in characters.by_ref() {
                if next == '}' {
                    closed = true;
                    break;
                }
                capture.push(next);
            }
            if !closed {
                output.push('{');
                output.push_str(&capture);
                continue;
            }
            if let Some(argument) = substitutions.get(&capture) {
                output.push('{');
                output.push_str(argument);
                output.push('}');
            } else {
                output.push('{');
                output.push_str(&capture);
                output.push('}');
            }
        } else if character == '@' {
            output.push('@');
            let mut rule_id = String::new();
            while let Some((_, next)) = characters.peek().copied() {
                if next == '_' || next == '-' || next.is_alphanumeric() {
                    rule_id.push(next);
                    output.push(next);
                    characters.next();
                } else {
                    break;
                }
            }
            if matches!(characters.peek(), Some((_, '('))) {
                characters.next();
                output.push('(');
                let mut raw_arguments = String::new();
                let mut closed = false;
                for (_, next) in characters.by_ref() {
                    if next == ')' {
                        closed = true;
                        break;
                    }
                    raw_arguments.push(next);
                }
                if !closed {
                    return Err(format!(
                        "nested syntax rule `@{rule_id}` has an unclosed parameter list"
                    ));
                }
                let raw_arguments = raw_arguments.trim();
                let arguments = if raw_arguments.is_empty() {
                    Vec::new()
                } else {
                    raw_arguments
                        .split(',')
                        .map(|argument| {
                            let trimmed = argument.trim();
                            substitutions
                                .get(trimmed)
                                .map_or(trimmed, |replacement| replacement.as_str())
                        })
                        .collect::<Vec<_>>()
                };
                output.push_str(&arguments.join(", "));
                output.push(')');
            }
        } else {
            output.push(character);
        }
        if output.len() > MAX_EXPANDED_BYTES {
            return Err(format!(
                "expanded pattern exceeds {MAX_EXPANDED_BYTES} bytes"
            ));
        }
    }
    Ok(output)
}

fn validate_field_constraints(
    constraints: &FieldConstraints,
    kind: &FieldKind,
    path: &str,
    errors: &mut Vec<SpecError>,
) {
    if constraints
        .min
        .zip(constraints.max)
        .is_some_and(|(min, max)| min > max)
    {
        errors.push(SpecError {
            code: "LM1043",
            message: format!("minimum numeric bound exceeds maximum for field `{path}`"),
        });
    }
    if constraints
        .min_length
        .zip(constraints.max_length)
        .is_some_and(|(min, max)| min > max)
    {
        errors.push(SpecError {
            code: "LM1044",
            message: format!("minimum text length exceeds maximum for field `{path}`"),
        });
    }
    if constraints
        .min_items
        .zip(constraints.max_items)
        .is_some_and(|(min, max)| min > max)
    {
        errors.push(SpecError {
            code: "LM1045",
            message: format!("minimum item count exceeds maximum for field `{path}`"),
        });
    }
    let number_bounds = constraints.min.is_some() || constraints.max.is_some();
    let length_bounds = constraints.min_length.is_some() || constraints.max_length.is_some();
    let item_bounds = constraints.min_items.is_some() || constraints.max_items.is_some();
    let valid_kinds = (!number_bounds || matches!(kind, FieldKind::Number))
        && (!length_bounds || matches!(kind, FieldKind::Text))
        && (!item_bounds
            || matches!(
                kind,
                FieldKind::List { .. } | FieldKind::Map { .. } | FieldKind::Properties { .. }
            ));
    if !valid_kinds {
        errors.push(SpecError {
            code: "LM1047",
            message: format!("constraints do not apply to the type of field `{path}`"),
        });
    }
}

fn default_satisfies_constraints(
    default: &str,
    kind: &FieldKind,
    constraints: &FieldConstraints,
) -> bool {
    match kind {
        FieldKind::Number => {
            let Ok(value) = default.parse::<f64>() else {
                return false;
            };
            constraints.min.is_none_or(|min| value >= min as f64)
                && constraints.max.is_none_or(|max| value <= max as f64)
        }
        FieldKind::Text => {
            let length = default.chars().count();
            constraints.min_length.is_none_or(|min| length >= min)
                && constraints.max_length.is_none_or(|max| length <= max)
        }
        FieldKind::Identifier
        | FieldKind::Enum { .. }
        | FieldKind::NamedEnum { .. }
        | FieldKind::Reference { .. }
        | FieldKind::Properties { .. }
        | FieldKind::List { .. }
        | FieldKind::Map { .. } => true,
    }
}

fn validate_property_fields(fields: &[FieldSpec], owner: &str, errors: &mut Vec<SpecError>) {
    let mut names = BTreeSet::new();
    for field in fields {
        let path = format!("{owner}.{}", field.name);
        validate_field_constraints(&field.constraints, &field.kind, &path, errors);
        if !valid_identifier(&field.name) {
            errors.push(SpecError {
                code: "LM1037",
                message: format!("property `{path}` has an invalid field name"),
            });
        }
        if !names.insert(field.name.as_str()) {
            errors.push(SpecError {
                code: "LM1033",
                message: format!("properties `{owner}` repeat field `{}`", field.name),
            });
        }
        if let Some(default) = &field.default {
            let valid = match &field.kind {
                FieldKind::Identifier => valid_identifier(default),
                FieldKind::Number => default.parse::<f64>().is_ok_and(f64::is_finite),
                FieldKind::Text => true,
                FieldKind::Enum { values } | FieldKind::NamedEnum { values, .. } => {
                    values.contains(default)
                }
                FieldKind::Reference { .. }
                | FieldKind::Properties { .. }
                | FieldKind::List { .. }
                | FieldKind::Map { .. } => false,
            };
            if !valid {
                errors.push(SpecError {
                    code: "LM1034",
                    message: format!("default `{default}` is invalid for property `{path}`"),
                });
            } else if !default_satisfies_constraints(default, &field.kind, &field.constraints) {
                errors.push(SpecError {
                    code: "LM1046",
                    message: format!(
                        "default `{default}` violates constraints for property `{path}`"
                    ),
                });
            }
        }
        if let FieldKind::Enum { values } = &field.kind {
            let mut enum_values = BTreeSet::new();
            if values.is_empty() {
                errors.push(SpecError {
                    code: "LM1035",
                    message: format!("enum property `{path}` has no values"),
                });
            }
            for value in values {
                if !valid_identifier(value) || !enum_values.insert(value.as_str()) {
                    errors.push(SpecError {
                        code: "LM1036",
                        message: format!(
                            "enum property `{path}` has invalid or duplicate value `{value}`"
                        ),
                    });
                }
            }
        }
        if let FieldKind::Properties { fields, .. } = &field.kind {
            validate_property_fields(fields, &path, errors);
        }
        if let FieldKind::List { item } = &field.kind {
            validate_list_item(item, &path, errors);
        }
        if let FieldKind::Map { key, value } = &field.kind {
            validate_map(key, value, &path, errors);
        }
    }
}

fn validate_named_type_usage(
    kind: &FieldKind,
    named_types: &BTreeMap<&str, &[FieldSpec]>,
    named_enums: &BTreeMap<&str, &[String]>,
    path: &str,
    errors: &mut Vec<SpecError>,
) {
    match kind {
        FieldKind::Properties { name, fields } => {
            if let Some(name) = name {
                match named_types.get(name.as_str()) {
                    Some(declared) if *declared == fields.as_slice() => {}
                    Some(_) => errors.push(SpecError {
                        code: "LM1057",
                        message: format!(
                            "named record `{name}` used at `{path}` does not match its declaration"
                        ),
                    }),
                    None => errors.push(SpecError {
                        code: "LM1058",
                        message: format!("field `{path}` refers to unknown named record `{name}`"),
                    }),
                }
            }
            for field in fields {
                validate_named_type_usage(
                    &field.kind,
                    named_types,
                    named_enums,
                    &format!("{path}.{}", field.name),
                    errors,
                );
            }
        }
        FieldKind::List { item } => {
            validate_named_type_usage(item, named_types, named_enums, path, errors)
        }
        FieldKind::Map { value, .. } => {
            validate_named_type_usage(value, named_types, named_enums, path, errors);
        }
        FieldKind::NamedEnum { name, values } => match named_enums.get(name.as_str()) {
            Some(declared) if *declared == values.as_slice() => {}
            Some(_) => errors.push(SpecError {
                code: "LM1068",
                message: format!(
                    "named enum `{name}` used at `{path}` does not match its declaration"
                ),
            }),
            None => errors.push(SpecError {
                code: "LM1069",
                message: format!("field `{path}` refers to unknown named enum `{name}`"),
            }),
        },
        FieldKind::Identifier
        | FieldKind::Number
        | FieldKind::Text
        | FieldKind::Enum { .. }
        | FieldKind::Reference { .. } => {}
    }
}

fn validate_map(_key: &MapKeyKind, value: &FieldKind, path: &str, errors: &mut Vec<SpecError>) {
    match value {
        FieldKind::Properties { fields, .. } => validate_property_fields(fields, path, errors),
        FieldKind::List { item } => validate_list_item(item, path, errors),
        FieldKind::Map { key, value } => validate_map(key, value, path, errors),
        FieldKind::Identifier
        | FieldKind::Number
        | FieldKind::Text
        | FieldKind::Enum { .. }
        | FieldKind::NamedEnum { .. }
        | FieldKind::Reference { .. } => {}
    }
    if let FieldKind::Enum { values } = value {
        if values.is_empty() {
            errors.push(SpecError {
                code: "LM1048",
                message: format!("map enum `{path}` has no values"),
            });
        }
        let mut seen = BTreeSet::new();
        for value in values {
            if !valid_identifier(value) || !seen.insert(value.as_str()) {
                errors.push(SpecError {
                    code: "LM1049",
                    message: format!("map enum `{path}` has invalid or duplicate value `{value}`"),
                });
            }
        }
    }
}

fn validate_list_item(kind: &FieldKind, path: &str, errors: &mut Vec<SpecError>) {
    match kind {
        FieldKind::Properties { fields, .. } => validate_property_fields(fields, path, errors),
        FieldKind::List { item } => validate_list_item(item, path, errors),
        FieldKind::Map { key, value } => validate_map(key, value, path, errors),
        FieldKind::Identifier
        | FieldKind::Number
        | FieldKind::Text
        | FieldKind::Enum { .. }
        | FieldKind::NamedEnum { .. }
        | FieldKind::Reference { .. } => {}
    }
    if let FieldKind::Enum { values } = kind {
        if values.is_empty() {
            errors.push(SpecError {
                code: "LM1044",
                message: format!("list enum `{path}` has no values"),
            });
        }
        let mut seen = BTreeSet::new();
        for value in values {
            if !valid_identifier(value) || !seen.insert(value.as_str()) {
                errors.push(SpecError {
                    code: "LM1045",
                    message: format!("list enum `{path}` has invalid or duplicate value `{value}`"),
                });
            }
        }
    }
    if let FieldKind::Properties { fields, .. } = kind {
        validate_property_fields(fields, path, errors);
    }
}

fn valid_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_alphabetic())
        && characters
            .all(|character| character == '_' || character == '-' || character.is_alphanumeric())
}

fn root_requires_block(constructs: &[ConstructSpec], root: &str) -> bool {
    constructs
        .iter()
        .find(|construct| construct.id == root)
        .is_some_and(|construct| construct.block.is_none() && construct.custom_parser.is_none())
}

fn can_contain_construct(
    constructs: &BTreeMap<&str, &ConstructSpec>,
    ancestor: &str,
    descendant: &str,
) -> bool {
    let mut pending = vec![ancestor];
    let mut visited = BTreeSet::new();
    while let Some(parent) = pending.pop() {
        if !visited.insert(parent) {
            continue;
        }
        if let Some(block) = constructs
            .get(parent)
            .and_then(|construct| construct.block.as_ref())
        {
            for child in &block.allowed {
                if child == descendant {
                    return true;
                }
                if constructs.contains_key(child.as_str()) {
                    pending.push(child);
                } else {
                    for (construct_id, construct) in constructs {
                        if construct.implements.iter().any(|item| item == child) {
                            if *construct_id == descendant {
                                return true;
                            }
                            pending.push(construct_id);
                        }
                    }
                }
            }
        }
    }
    false
}
