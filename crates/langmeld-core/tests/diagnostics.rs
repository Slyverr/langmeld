use langmeld_core::{
    Diagnostic, Severity, SourceDatabase, SourceFile, Span, format_diagnostics, has_errors,
};

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
        assert_eq!(location.source_name(), source.name());
        assert_eq!(location.span(), span);
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
fn detects_errors_without_treating_warnings_or_information_as_errors() {
    let non_errors = [
        Diagnostic::spec_warning("TEST-001", "warning"),
        Diagnostic::spec_info("TEST-002", "information"),
    ];
    let errors = [
        Diagnostic::spec_warning("TEST-003", "warning"),
        Diagnostic::spec_error("TEST-004", "error"),
        Diagnostic::spec_info("TEST-005", "information"),
    ];

    assert!(!has_errors(&non_errors));
    assert!(has_errors(&errors));
}

#[test]
fn formats_source_locations_using_unicode_scalar_columns() {
    let source = SourceFile::new("main.lang", "first\nérror\n");
    let diagnostic = Diagnostic::error("TEST-001", "invalid token", &source, Span::new(8, 12));

    assert_eq!(
        format_diagnostics(&[source], &[diagnostic]),
        "main.lang:2:2: error[TEST-001]: invalid token"
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
        "missing.lang:1:1: error[TEST-001]: source not available\n<project>:1:1: error[TEST-002]: invalid project setting"
    );
}

#[test]
fn formats_warning_and_info_severities() {
    let source = SourceFile::new("main.lang", "text");
    let diagnostics = [
        Diagnostic::warning("TEST-001", "warning", &source, Span::new(0, 1)),
        Diagnostic::info("TEST-002", "information", &source, Span::new(1, 2)),
    ];

    assert_eq!(
        format_diagnostics(&[source], &diagnostics),
        "main.lang:1:1: warning[TEST-001]: warning\nmain.lang:1:2: info[TEST-002]: information"
    );
}

#[test]
fn formats_locations_against_matching_identity_when_names_collide() {
    let mut first_database = SourceDatabase::default();
    let first_id = first_database.insert("shared.lang", "abcdefg").unwrap();
    let first_source = first_database.get(first_id).unwrap().clone();

    let mut second_database = SourceDatabase::default();
    let second_id = second_database.insert("shared.lang", "abc\ndef").unwrap();
    let second_source = second_database.get(second_id).unwrap().clone();
    let diagnostic =
        Diagnostic::error("TEST-001", "invalid token", &second_source, Span::new(4, 7));

    assert_eq!(
        format_diagnostics(&[first_source, second_source], &[diagnostic]),
        "shared.lang:2:1: error[TEST-001]: invalid token"
    );
}

#[test]
fn formats_standalone_locations_by_identity_when_names_collide() {
    let first_source = SourceFile::new("shared.lang", "abcdefg");
    let second_source = SourceFile::new("shared.lang", "abc\ndef");
    let diagnostic =
        Diagnostic::error("TEST-001", "invalid token", &second_source, Span::new(4, 7));

    assert_eq!(
        format_diagnostics(&[first_source, second_source], &[diagnostic]),
        "shared.lang:2:1: error[TEST-001]: invalid token"
    );
}

#[test]
fn does_not_format_stale_standalone_location_against_a_same_name_source() {
    let old_source = SourceFile::new("shared.lang", "first\ninvalid");
    let diagnostic = Diagnostic::error("TEST-001", "invalid token", &old_source, Span::new(6, 13));
    let current_source = SourceFile::new("shared.lang", "replacement");

    assert_eq!(
        format_diagnostics(&[current_source], &[diagnostic]),
        "shared.lang:1:1: error[TEST-001]: invalid token"
    );
}

#[test]
fn does_not_format_managed_location_against_a_different_revision() {
    let mut database = SourceDatabase::default();
    let source_id = database.insert("main.lang", "abcdefg").unwrap();
    let old_source = database.get(source_id).unwrap().clone();
    let diagnostic = Diagnostic::error("TEST-001", "invalid token", &old_source, Span::new(4, 5));

    database.update(source_id, "a\nb\ncdef").unwrap();
    let current_source = database.get(source_id).unwrap().clone();

    assert_eq!(
        format_diagnostics(&[current_source], &[diagnostic]),
        "main.lang:1:1: error[TEST-001]: invalid token"
    );
}
