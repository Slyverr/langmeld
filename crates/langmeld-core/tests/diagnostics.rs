use langmeld_core::{Diagnostic, SourceFile, Span, format_diagnostics};

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
