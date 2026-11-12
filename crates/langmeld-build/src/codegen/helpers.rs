pub(super) const HELPERS: &str = r#"
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
