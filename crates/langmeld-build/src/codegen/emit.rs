use crate::BuildError;

use super::{
    ExpectedCapture, FieldShape, GeneratedConstruct, GeneratedVariant, GroupKind, GroupShape,
    ScopeShape, group_type_name, helpers::HELPERS, unsupported,
};

pub(super) fn bindings(constructs: &[GeneratedConstruct]) -> Result<String, BuildError> {
    let mut output = String::from("#[allow(dead_code, missing_docs)]\npub mod constructs {\n\n");
    output.push_str(HELPERS);

    for construct in constructs {
        emit_construct(&mut output, construct)?;
    }

    output.push_str("}\n");

    Ok(output)
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
        r#"    impl<'source> ::langmeld_language::TypedConstruct<'source> for {}<'source> {{
        const CONSTRUCT: &'static str = {:?};

        fn from_validated_construct(source: &'source ::langmeld_core::SourceFile, construct: &'source ::langmeld_language::ParsedConstruct) -> ::core::result::Result<Self, ::langmeld_core::Diagnostic> {{
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
        r#"    impl<'source> ::langmeld_language::TypedConstruct<'source> for {}<'source> {{
        const CONSTRUCT: &'static str = {:?};

        fn from_validated_construct(source: &'source ::langmeld_core::SourceFile, construct: &'source ::langmeld_language::ParsedConstruct) -> ::core::result::Result<Self, ::langmeld_core::Diagnostic> {{
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
        source: &'source ::langmeld_core::SourceFile,
        construct: &'source ::langmeld_language::ParsedConstruct,
        fields: &'source [::langmeld_language::CapturedField],
        groups: &'source [::langmeld_language::CaptureGroup],
        span: ::langmeld_core::Span,
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
            "::langmeld_language::CapturedIdentifier<'source>".to_owned(),
            format!(
                "__capture_identifier(self.source, self.construct, self.fields, {:?}, self.span)",
                field.name
            ),
        ),
        ExpectedCapture::Token => (
            "::langmeld_language::CapturedText<'source>".to_owned(),
            text_access("Token"),
        ),
        ExpectedCapture::Integer => (
            "::langmeld_language::CapturedText<'source>".to_owned(),
            text_access("Integer"),
        ),
        ExpectedCapture::String => (
            "::langmeld_language::CapturedText<'source>".to_owned(),
            text_access("String"),
        ),
        ExpectedCapture::Expression => (
            "::langmeld_language::CapturedText<'source>".to_owned(),
            text_access("Expression"),
        ),
    };

    output.push_str(&format!(
        r#"        pub fn {}(&self) -> ::core::result::Result<{return_type}, ::langmeld_core::Diagnostic> {{
            {access}
        }}

"#,
        field.rust_name
    ));
}

fn emit_group_method(output: &mut String, group: &GroupShape, group_type: &str) {
    let (expected_kind, return_type, collection) = match group.kind {
        GroupKind::Optional => (
            "Optional",
            format!("::core::option::Option<{group_type}<'source>>"),
            false,
        ),
        GroupKind::Repeated => (
            "Repeat",
            format!("::std::vec::Vec<{group_type}<'source>>"),
            true,
        ),
    };

    output.push_str(&format!(
        r#"        pub fn {}(&self) -> ::core::result::Result<{return_type}, ::langmeld_core::Diagnostic> {{
            let group = __group(
                self.source,
                self.construct,
                self.groups,
                {:?},
                self.span,
                ::langmeld_language::CaptureGroupKind::{expected_kind},
            )?;
"#,
        group.rust_name, group.name
    ));

    if collection {
        output.push_str(&format!(
            r#"            let mut views = ::std::vec::Vec::with_capacity(group.occurrences.len());
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
