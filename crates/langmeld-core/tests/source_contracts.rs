use langmeld_core::{Diagnostic, Severity, SourceFile, Span};

#[test]
fn source_positions_count_unicode_scalars_and_clamp_invalid_offsets() {
    let source = SourceFile::new("unicode.lang", "a🦀z\nnext");

    assert_eq!(source.line_column(0), (1, 1));
    assert_eq!(source.line_column(1), (1, 2));
    assert_eq!(source.line_column(2), (1, 2));
    assert_eq!(source.line_column(5), (1, 3));
    assert_eq!(source.line_column(usize::MAX), (2, 5));
}

#[test]
fn diagnostics_retain_optional_spans_and_serialize_severity_stably() {
    let located = Diagnostic::error("LM1001", "unexpected token", Span::new(2, 3));
    let unlocated = Diagnostic::spec_error("LM0001", "invalid definition");

    assert_eq!(located.severity, Severity::Error);
    assert_eq!(located.span, Some(Span::new(2, 3)));
    assert_eq!(unlocated.span, None);

    let json = serde_json::to_value(&located).expect("diagnostic serializes");
    assert_eq!(json["severity"], "error");
    assert_eq!(json["span"]["start"], 2);
    assert_eq!(json["span"]["end"], 3);
}
