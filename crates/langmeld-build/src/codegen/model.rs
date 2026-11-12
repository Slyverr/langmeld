use std::collections::HashSet;

use langmeld_language::{FieldKind, LanguageSpec, PatternPart, RepeatMinimum};

use crate::BuildError;

use super::{
    ExpectedCapture, FieldShape, GeneratedConstruct, GeneratedVariant, GroupKind, GroupShape,
    ScopeShape, group_type_name, register_method, register_type, rust_field_name, rust_type_name,
    unsupported,
};

pub(super) fn build(specification: &LanguageSpec) -> Result<Vec<GeneratedConstruct>, BuildError> {
    let mut type_names = HashSet::new();
    let mut constructs = Vec::new();

    for construct in specification.constructs() {
        let construct_name = rust_type_name(construct.name())?;
        register_type(&mut type_names, &construct_name)?;

        if construct.variants().len() == 1 {
            let shape = scope_shape(construct.pattern(), construct.name())?;
            if shape
                .fields
                .iter()
                .any(|field| field.rust_name == "from_construct")
                || shape
                    .groups
                    .iter()
                    .any(|group| group.rust_name == "from_construct")
            {
                return Err(unsupported(format!(
                    "construct `{}` has a top-level capture or group named `from_construct`, which conflicts with the generated typed-view constructor; rename it or use the dynamic API",
                    construct.name()
                )));
            }

            constructs.push(GeneratedConstruct {
                name: construct.name().to_owned(),
                type_name: construct_name.clone(),
                variants: vec![GeneratedVariant {
                    index: 0,
                    enum_variant: None,
                    view_type: construct_name,
                    shape,
                }],
            });
            continue;
        }

        let mut variants = Vec::new();
        for (index, pattern) in construct.variants().iter().enumerate() {
            let Some(variant_name) = construct.variant_name(index) else {
                return Err(unsupported(format!(
                    "construct `{}` has multiple syntax variants; name each with `syntax \"variant_name\"` to generate typed Rust bindings, or use the dynamic API",
                    construct.name()
                )));
            };
            let enum_variant = rust_type_name(variant_name)?;
            let view_type = rust_type_name(&format!("{} {variant_name}", construct.name()))?;

            register_type(&mut type_names, &view_type)?;
            variants.push(GeneratedVariant {
                index,
                enum_variant: Some(enum_variant),
                view_type,
                shape: scope_shape(pattern, construct.name())?,
            });
        }

        constructs.push(GeneratedConstruct {
            name: construct.name().to_owned(),
            type_name: construct_name,
            variants,
        });
    }

    for construct in &constructs {
        for variant in &construct.variants {
            register_group_types(&mut type_names, &variant.view_type, &variant.shape)?;
        }
    }

    Ok(constructs)
}

fn scope_shape(parts: &[PatternPart], construct: &str) -> Result<ScopeShape, BuildError> {
    let mut fields = Vec::new();
    let mut groups = Vec::new();
    let mut method_names = HashSet::new();

    for part in parts {
        match part {
            PatternPart::Literal(_) => {}
            PatternPart::Field { name, kind } => {
                let rust_name = rust_field_name(name)?;
                register_method(&mut method_names, &rust_name, name, construct)?;
                let expected = match kind {
                    FieldKind::Token => ExpectedCapture::Token,
                    FieldKind::Identifier => ExpectedCapture::Identifier,
                    FieldKind::Integer => ExpectedCapture::Integer,
                    FieldKind::String => ExpectedCapture::String,
                };
                fields.push(FieldShape {
                    name: name.clone(),
                    rust_name,
                    expected,
                });
            }
            PatternPart::Choice { name, .. } => {
                push_text_field(
                    &mut fields,
                    &mut method_names,
                    name,
                    ExpectedCapture::Token,
                    construct,
                )?;
            }
            PatternPart::Expression { name, .. } => {
                push_text_field(
                    &mut fields,
                    &mut method_names,
                    name,
                    ExpectedCapture::Expression,
                    construct,
                )?;
            }
            PatternPart::Optional { name, parts } => {
                groups.push(group_shape(
                    name.as_deref(),
                    parts,
                    GroupKind::Optional,
                    &mut method_names,
                    construct,
                )?);
            }
            PatternPart::Repeat {
                name,
                parts,
                minimum: RepeatMinimum::Zero | RepeatMinimum::One,
            } => {
                groups.push(group_shape(
                    name.as_deref(),
                    parts,
                    GroupKind::Repeated,
                    &mut method_names,
                    construct,
                )?);
            }
        }
    }

    Ok(ScopeShape { fields, groups })
}

fn group_shape(
    name: Option<&str>,
    parts: &[PatternPart],
    kind: GroupKind,
    method_names: &mut HashSet<String>,
    construct: &str,
) -> Result<GroupShape, BuildError> {
    let Some(name) = name else {
        return Err(unsupported(format!(
            "construct `{construct}` has an unnamed optional or repeated group; name it with `optional \"name\"`, `zero_or_more \"name\"`, or `one_or_more \"name\"` to generate typed Rust bindings, or use `ParsedConstruct::capture_groups` through the dynamic API"
        )));
    };
    let rust_name = rust_field_name(name)?;
    register_method(method_names, &rust_name, name, construct)?;
    let shape = scope_shape(parts, construct)?;

    Ok(GroupShape {
        name: name.to_owned(),
        rust_name,
        kind,
        shape,
    })
}

fn push_text_field(
    fields: &mut Vec<FieldShape>,
    method_names: &mut HashSet<String>,
    name: &str,
    expected: ExpectedCapture,
    construct: &str,
) -> Result<(), BuildError> {
    let rust_name = rust_field_name(name)?;
    register_method(method_names, &rust_name, name, construct)?;
    fields.push(FieldShape {
        name: name.to_owned(),
        rust_name,
        expected,
    });
    Ok(())
}

fn register_group_types(
    type_names: &mut HashSet<String>,
    parent: &str,
    shape: &ScopeShape,
) -> Result<(), BuildError> {
    for group in &shape.groups {
        let type_name = group_type_name(parent, &group.name)?;
        register_type(type_names, &type_name)?;
        register_group_types(type_names, &type_name, &group.shape)?;
    }

    Ok(())
}
