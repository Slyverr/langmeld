use langmeld_core::{Diagnostic, Severity, SourceFile, Span, format_diagnostics};

#[test]
fn constructs_source_and_project_diagnostics_at_every_severity() {
    let source = SourceFile::new("main.lang", "text");
    let span = Span::new(1, 3);
    let source_diagnostics = [
        Diagnostic::error("TEST-001", "error", &source, span),
        Diagnostic::warning("TEST-002", "warning", &source, span),
        Diagnostic::info("TEST-003", "info", &source, span),
    ];
    let project_diagnostics = [
        Diagnostic::spec_error("TEST-004", "error"),
        Diagnostic::spec_warning("TEST-005", "warning"),
        Diagnostic::spec_info("TEST-006", "info"),
    ];

    for (diagnostic, severity) in
        source_diagnostics
            .iter()
            .zip([Severity::Error, Severity::Warning, Severity::Info])
    {
        assert_eq!(diagnostic.severity, severity);
        let location = diagnostic.location.as_ref().expect("source location");
        assert_eq!(location.source_name, source.name);
        assert_eq!(location.span, span);
    }

    for (diagnostic, severity) in
        project_diagnostics
            .iter()
            .zip([Severity::Error, Severity::Warning, Severity::Info])
    {
        assert_eq!(diagnostic.severity, severity);
        assert!(diagnostic.location.is_none());
    }
}

#[test]
fn formats_source_locations_using_unicode_scalar_columns() {
    let source = SourceFile::new("main.lang", "first\nérror\n");
    let diagnostic = Diagnostic::error("TEST-001", "invalid token", &source, Span::new(8, 12));

    assert_eq!(
        format_diagnostics(&[source], &[diagnostic]),
        "main.lang:2:2: TEST-001: invalid token"
    );
}

#[test]
fn formats_missing_sources_and_project_level_diagnostics_with_safe_locations() {
    let missing_source = SourceFile::new("missing.lang", "text");
    let diagnostics = [
        Diagnostic::error(
            "TEST-001",
            "source not available",
            &missing_source,
            Span::new(2, 3),
        ),
        Diagnostic::spec_error("TEST-002", "invalid project setting"),
    ];

    assert_eq!(
        format_diagnostics(&[], &diagnostics),
        "missing.lang:1:1: TEST-001: source not available\n<project>:1:1: TEST-002: invalid project setting"
    );
}
