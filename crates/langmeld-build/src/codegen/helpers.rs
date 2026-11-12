pub(super) const HELPERS: &str = r#"
    #[derive(Clone, Copy)]
    enum __ExpectedCapture {
        Token,
        Integer,
        String,
        Expression,
    }

    fn __error(
        source: &::langmeld_core::SourceFile,
        construct: &str,
        detail: &str,
        span: ::langmeld_core::Span,
    ) -> ::langmeld_core::Diagnostic {
        ::langmeld_core::Diagnostic::error(
            "LM-TYPED-001",
            format!("typed view for `{construct}` is incompatible with this parse: {detail}"),
            source,
            span,
        )
    }

    fn __capture_text<'source>(
        source: &'source ::langmeld_core::SourceFile,
        construct: &::langmeld_language::ParsedConstruct,
        fields: &'source [::langmeld_language::CapturedField],
        name: &str,
        scope_span: ::langmeld_core::Span,
        expected: __ExpectedCapture,
    ) -> ::core::result::Result<
        ::langmeld_language::CapturedText<'source>,
        ::langmeld_core::Diagnostic,
    > {
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
            (__ExpectedCapture::Token, ::langmeld_language::CapturedValue::Token { .. })
                | (
                    __ExpectedCapture::Integer,
                    ::langmeld_language::CapturedValue::Integer(_),
                )
                | (
                    __ExpectedCapture::String,
                    ::langmeld_language::CapturedValue::String(_),
                )
                | (
                    __ExpectedCapture::Expression,
                    ::langmeld_language::CapturedValue::Expression,
                )
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

        Ok(::langmeld_language::CapturedText {
            text,
            span: field.span,
        })
    }

    fn __capture_identifier<'source>(
        source: &'source ::langmeld_core::SourceFile,
        construct: &::langmeld_language::ParsedConstruct,
        fields: &'source [::langmeld_language::CapturedField],
        name: &str,
        scope_span: ::langmeld_core::Span,
    ) -> ::core::result::Result<
        ::langmeld_language::CapturedIdentifier<'source>,
        ::langmeld_core::Diagnostic,
    > {
        let Some(field) = fields.iter().find(|field| field.name == name) else {
            return Err(__error(
                source,
                construct.name.as_str(),
                &format!("required capture `{name}` is absent"),
                scope_span,
            ));
        };

        let ::langmeld_language::CapturedValue::Identifier(text) = &field.value else {
            return Err(__error(
                source,
                construct.name.as_str(),
                &format!("capture `{name}` is not an identifier"),
                field.span,
            ));
        };

        Ok(::langmeld_language::CapturedIdentifier {
            text,
            span: field.span,
        })
    }

    fn __group<'a>(
        source: &::langmeld_core::SourceFile,
        construct: &::langmeld_language::ParsedConstruct,
        groups: &'a [::langmeld_language::CaptureGroup],
        name: &str,
        scope_span: ::langmeld_core::Span,
        expected: ::langmeld_language::CaptureGroupKind,
    ) -> ::core::result::Result<
        &'a ::langmeld_language::CaptureGroup,
        ::langmeld_core::Diagnostic,
    > {
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
        source: &::langmeld_core::SourceFile,
        construct: &'source ::langmeld_language::ParsedConstruct,
        occurrence: &'source ::langmeld_language::CaptureGroupOccurrence,
    ) -> ::core::result::Result<
        &'source [::langmeld_language::CapturedField],
        ::langmeld_core::Diagnostic,
    > {
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
