use std::collections::HashSet;

use heck::{ToSnakeCase, ToUpperCamelCase};
use langmeld_language::{FieldKind, LanguageSpec, PatternPart, RepeatMinimum};

use crate::BuildError;

/// Generates typed Rust construct views for a validated language grammar.
///
/// Construct and capture names become Rust type and method names. All
/// optional and repeated groups must have explicit names. Constructs with
/// multiple syntax variants must name every variant with `syntax "name"`.
///
/// # Errors
///
/// Returns an error if names cannot map unambiguously to Rust identifiers or
/// the grammar contains unnamed groups or syntax variants.
/// A single-variant construct also cannot have a top-level capture or group
/// that maps to `from_construct`, which is the generated view constructor.
pub fn generate_bindings(specification: &LanguageSpec) -> Result<String, BuildError> {
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
                    "construct `{}` has multiple syntax variants; name each with `syntax \"variant_name\"` before generating typed Rust bindings",
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

    let mut output = String::from(
        "#[allow(dead_code, missing_docs)]\npub mod constructs {\n    use langmeld_core::{Diagnostic, SourceFile, Span};\n    use langmeld_language::{\n        CaptureGroup, CaptureGroupKind, CapturedField, CapturedIdentifier, CapturedText,\n        CapturedValue, ParsedConstruct, TypedConstruct,\n    };\n\n",
    );
    output.push_str(HELPERS);

    for construct in &constructs {
        emit_construct(&mut output, construct)?;
    }

    output.push_str("}\n");
    Ok(output)
}

struct GeneratedConstruct {
    name: String,
    type_name: String,
    variants: Vec<GeneratedVariant>,
}

struct GeneratedVariant {
    index: usize,
    enum_variant: Option<String>,
    view_type: String,
    shape: ScopeShape,
}

struct ScopeShape {
    fields: Vec<FieldShape>,
    groups: Vec<GroupShape>,
}

struct FieldShape {
    name: String,
    rust_name: String,
    expected: ExpectedCapture,
}

struct GroupShape {
    name: String,
    rust_name: String,
    kind: GroupKind,
    shape: ScopeShape,
}

#[derive(Clone, Copy)]
enum ExpectedCapture {
    Identifier,
    Token,
    Integer,
    String,
    Expression,
}

#[derive(Clone, Copy)]
enum GroupKind {
    Optional,
    Repeated,
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
            "construct `{construct}` has an unnamed optional or repeated group; name it with `optional \"name\"` or `zero_or_more \"name\"` to generate typed Rust bindings"
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

fn emit_construct(output: &mut String, construct: &GeneratedConstruct) -> Result<(), BuildError> {
    if construct.variants.len() == 1 {
        emit_view(
            output,
            &construct.variants[0].view_type,
            &construct.variants[0].shape,
        )?;
        emit_single_variant_impl(output, construct, &construct.variants[0]);
        return Ok(());
    }

    output.push_str(&format!(
        "    #[derive(Clone, Copy, Debug)]\n    pub enum {}<'source> {{\n",
        construct.type_name
    ));
    for variant in &construct.variants {
        let Some(enum_variant) = &variant.enum_variant else {
            return Err(unsupported("internal variant name is missing".to_owned()));
        };
        output.push_str(&format!(
            "        {enum_variant}({}<'source>),\n",
            variant.view_type
        ));
    }
    output.push_str("    }\n\n");

    for variant in &construct.variants {
        emit_view(output, &variant.view_type, &variant.shape)?;
    }

    output.push_str(&format!(
        r#"    impl<'source> TypedConstruct<'source> for {}<'source> {{
        const CONSTRUCT: &'static str = {:?};

        fn from_validated_construct(source: &'source SourceFile, construct: &'source ParsedConstruct) -> Result<Self, Diagnostic> {{
            match construct.variant {{
"#,
        construct.type_name, construct.name
    ));
    for variant in &construct.variants {
        let enum_variant = variant
            .enum_variant
            .as_deref()
            .ok_or_else(|| unsupported("internal variant name is missing".to_owned()))?;
        output.push_str(&format!(
            "                    {} => Ok(Self::{enum_variant}({} {{\n",
            variant.index, variant.view_type
        ));
        emit_construct_view_fields(output, "                        ");
        output.push_str("                    })),\n");
    }
    output.push_str(
        "                _ => Err(__error(source, construct.name.as_str(), \"syntax variant is not present in generated bindings\", construct.span)),\n            }\n        }\n    }\n\n",
    );

    Ok(())
}

fn emit_single_variant_impl(
    output: &mut String,
    construct: &GeneratedConstruct,
    variant: &GeneratedVariant,
) {
    output.push_str(&format!(
        r#"    impl<'source> TypedConstruct<'source> for {}<'source> {{
        const CONSTRUCT: &'static str = {:?};

        fn from_validated_construct(source: &'source SourceFile, construct: &'source ParsedConstruct) -> Result<Self, Diagnostic> {{
            if construct.variant != {} {{
                return Err(__error(source, construct.name.as_str(), "syntax variant is not present in generated bindings", construct.span));
            }}

            Ok(Self {{
"#,
        construct.type_name, construct.name, variant.index
    ));
    emit_construct_view_fields(output, "                ");
    output.push_str("            })\n        }\n    }\n\n");
}

fn emit_construct_view_fields(output: &mut String, indentation: &str) {
    for field in [
        "source,\n",
        "construct,\n",
        "fields: &construct.fields,\n",
        "groups: &construct.capture_groups,\n",
        "span: construct.span,\n",
    ] {
        output.push_str(indentation);
        output.push_str(field);
    }
}

fn emit_view(output: &mut String, type_name: &str, shape: &ScopeShape) -> Result<(), BuildError> {
    emit_scope_view(output, type_name, shape)
}

fn emit_group_view(
    output: &mut String,
    shape: &ScopeShape,
    type_name: &str,
) -> Result<(), BuildError> {
    emit_scope_view(output, type_name, shape)
}

fn emit_scope_view(
    output: &mut String,
    type_name: &str,
    shape: &ScopeShape,
) -> Result<(), BuildError> {
    output.push_str(&format!(
        r#"    #[derive(Clone, Copy, Debug)]
    pub struct {type_name}<'source> {{
        source: &'source SourceFile,
        construct: &'source ParsedConstruct,
        fields: &'source [CapturedField],
        groups: &'source [CaptureGroup],
        span: Span,
    }}

    impl<'source> {type_name}<'source> {{
"#
    ));

    for field in &shape.fields {
        emit_field_method(output, field);
    }

    for group in &shape.groups {
        let nested_type = group_type_name(type_name, &group.name)?;
        emit_group_method(output, group, &nested_type);
    }

    output.push_str("    }\n\n");

    for group in &shape.groups {
        let nested_type = group_type_name(type_name, &group.name)?;
        emit_group_view(output, &group.shape, &nested_type)?;
    }

    Ok(())
}

fn emit_field_method(output: &mut String, field: &FieldShape) {
    let text_access = |expected: &str| {
        format!(
            "__capture_text(self.source, self.construct, self.fields, {:?}, self.span, __ExpectedCapture::{expected})",
            field.name
        )
    };
    let (return_type, access) = match field.expected {
        ExpectedCapture::Identifier => (
            "CapturedIdentifier<'source>".to_owned(),
            format!(
                "__capture_identifier(self.source, self.construct, self.fields, {:?}, self.span)",
                field.name
            ),
        ),
        ExpectedCapture::Token => ("CapturedText<'source>".to_owned(), text_access("Token")),
        ExpectedCapture::Integer => ("CapturedText<'source>".to_owned(), text_access("Integer")),
        ExpectedCapture::String => ("CapturedText<'source>".to_owned(), text_access("String")),
        ExpectedCapture::Expression => (
            "CapturedText<'source>".to_owned(),
            text_access("Expression"),
        ),
    };

    output.push_str(&format!(
        r#"        pub fn {}(&self) -> Result<{return_type}, Diagnostic> {{
            {access}
        }}

"#,
        field.rust_name
    ));
}

fn emit_group_method(output: &mut String, group: &GroupShape, group_type: &str) {
    let (expected_kind, return_type, collection) = match group.kind {
        GroupKind::Optional => ("Optional", format!("Option<{group_type}<'source>>"), false),
        GroupKind::Repeated => ("Repeat", format!("Vec<{group_type}<'source>>"), true),
    };

    output.push_str(&format!(
        r#"        pub fn {}(&self) -> Result<{return_type}, Diagnostic> {{
            let group = __group(
                self.source,
                self.construct,
                self.groups,
                {:?},
                self.span,
                CaptureGroupKind::{expected_kind},
            )?;
"#,
        group.rust_name, group.name
    ));

    if collection {
        output.push_str(&format!(
            r#"            let mut views = Vec::with_capacity(group.occurrences.len());
            for occurrence in &group.occurrences {{
                let fields = __occurrence_fields(self.source, self.construct, occurrence)?;
                views.push({group_type} {{
                    source: self.source,
                    construct: self.construct,
                    fields,
                    groups: &occurrence.groups,
                    span: occurrence.span,
                }});
            }}
            Ok(views)
"#
        ));
    } else {
        output.push_str(&format!(
            r#"            match group.occurrences.as_slice() {{
                [] => Ok(None),
                [occurrence] => {{
                    let fields = __occurrence_fields(self.source, self.construct, occurrence)?;
                    Ok(Some({group_type} {{
                        source: self.source,
                        construct: self.construct,
                        fields,
                        groups: &occurrence.groups,
                        span: occurrence.span,
                    }}))
                }}
                _ => Err(__error(
                    self.source,
                    self.construct.name.as_str(),
                    "optional group matched more than once",
                    self.span,
                )),
            }}
"#
        ));
    }

    output.push_str("        }\n\n");
}

fn group_type_name(parent: &str, name: &str) -> Result<String, BuildError> {
    rust_type_name(&format!("{parent} {name}"))
}

fn rust_type_name(name: &str) -> Result<String, BuildError> {
    let mut result = name.to_upper_camel_case();
    if result.is_empty() {
        return Err(unsupported(format!(
            "`{name}` does not produce a valid Rust type name"
        )));
    }
    if !result
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
        || !result
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
    {
        return Err(unsupported(format!(
            "`{name}` cannot be converted to a portable Rust type name"
        )));
    }
    if result == "_" || is_keyword(&result) {
        result.push_str("Construct");
    }

    Ok(result)
}

fn rust_field_name(name: &str) -> Result<String, BuildError> {
    let mut result = name.to_snake_case();
    if result.is_empty()
        || !result
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        || !result
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
    {
        return Err(unsupported(format!(
            "`{name}` cannot be converted to a portable Rust field name"
        )));
    }
    if result == "_" || is_keyword(&result) {
        result.push('_');
    }

    Ok(result)
}

fn register_type(names: &mut HashSet<String>, name: &str) -> Result<(), BuildError> {
    if names.insert(name.to_owned()) {
        Ok(())
    } else {
        Err(unsupported(format!(
            "multiple language constructs or groups map to the Rust type `{name}`; rename them or use the dynamic API"
        )))
    }
}

fn register_method(
    names: &mut HashSet<String>,
    rust_name: &str,
    source_name: &str,
    construct: &str,
) -> Result<(), BuildError> {
    if names.insert(rust_name.to_owned()) {
        Ok(())
    } else {
        Err(unsupported(format!(
            "capture or group `{source_name}` in construct `{construct}` conflicts with another generated method `{rust_name}`"
        )))
    }
}

fn unsupported(message: String) -> BuildError {
    BuildError::UnsupportedGrammar(message)
}

fn is_keyword(name: &str) -> bool {
    matches!(
        name,
        "as" | "async"
            | "await"
            | "become"
            | "box"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "do"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "final"
            | "fn"
            | "for"
            | "gen"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "macro"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "override"
            | "priv"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "typeof"
            | "union"
            | "unsafe"
            | "unsized"
            | "use"
            | "virtual"
            | "where"
            | "while"
            | "yield"
    )
}

const HELPERS: &str = r#"
    #[derive(Clone, Copy)]
    enum __ExpectedCapture {
        Token,
        Integer,
        String,
        Expression,
    }

    fn __error(
        source: &SourceFile,
        construct: &str,
        detail: &str,
        span: Span,
    ) -> Diagnostic {
        Diagnostic::error(
            "LM-TYPED-001",
            format!("typed view for `{construct}` is incompatible with this parse: {detail}"),
            source,
            span,
        )
    }

    fn __capture_text<'source>(
        source: &'source SourceFile,
        construct: &ParsedConstruct,
        fields: &'source [CapturedField],
        name: &str,
        scope_span: Span,
        expected: __ExpectedCapture,
    ) -> Result<CapturedText<'source>, Diagnostic> {
        let Some(field) = fields.iter().find(|field| field.name == name) else {
            return Err(__error(
                source,
                construct.name.as_str(),
                &format!("required capture `{name}` is absent"),
                scope_span,
            ));
        };

        let matches = matches!(
            (expected, &field.value),
            (__ExpectedCapture::Token, CapturedValue::Token { .. })
                | (__ExpectedCapture::Integer, CapturedValue::Integer(_))
                | (__ExpectedCapture::String, CapturedValue::String(_))
                | (__ExpectedCapture::Expression, CapturedValue::Expression)
        );
        if !matches {
            return Err(__error(
                source,
                construct.name.as_str(),
                &format!("capture `{name}` has an unexpected lexical kind"),
                field.span,
            ));
        }

        let Some(text) = source.text().get(field.span.start..field.span.end) else {
            return Err(__error(
                source,
                construct.name.as_str(),
                &format!("capture `{name}` has an invalid source range"),
                field.span,
            ));
        };

        Ok(CapturedText { text, span: field.span })
    }

    fn __capture_identifier<'source>(
        source: &'source SourceFile,
        construct: &ParsedConstruct,
        fields: &'source [CapturedField],
        name: &str,
        scope_span: Span,
    ) -> Result<CapturedIdentifier<'source>, Diagnostic> {
        let Some(field) = fields.iter().find(|field| field.name == name) else {
            return Err(__error(
                source,
                construct.name.as_str(),
                &format!("required capture `{name}` is absent"),
                scope_span,
            ));
        };

        let CapturedValue::Identifier(text) = &field.value else {
            return Err(__error(
                source,
                construct.name.as_str(),
                &format!("capture `{name}` is not an identifier"),
                field.span,
            ));
        };

        Ok(CapturedIdentifier { text, span: field.span })
    }

    fn __group<'a>(
        source: &SourceFile,
        construct: &ParsedConstruct,
        groups: &'a [CaptureGroup],
        name: &str,
        scope_span: Span,
        expected: CaptureGroupKind,
    ) -> Result<&'a CaptureGroup, Diagnostic> {
        let Some(group) = groups.iter().find(|group| group.name.as_deref() == Some(name)) else {
            return Err(__error(
                source,
                construct.name.as_str(),
                &format!("required capture group `{name}` is absent"),
                scope_span,
            ));
        };
        if group.kind != expected {
            return Err(__error(
                source,
                construct.name.as_str(),
                &format!("capture group `{name}` has an unexpected structure"),
                scope_span,
            ));
        }

        Ok(group)
    }

    fn __occurrence_fields<'source>(
        source: &SourceFile,
        construct: &'source ParsedConstruct,
        occurrence: &'source langmeld_language::CaptureGroupOccurrence,
    ) -> Result<&'source [CapturedField], Diagnostic> {
        occurrence.captures(construct).ok_or_else(|| {
            __error(
                source,
                construct.name.as_str(),
                "capture group refers to fields outside its construct",
                occurrence.span,
            )
        })
    }
"#;
