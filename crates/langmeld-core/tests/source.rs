use langmeld_core::{
    Diagnostic, PositionEncoding, SourceDatabase, SourceDatabaseError, SourceFile, SourcePosition,
    Span,
};

#[test]
fn line_index_converts_utf8_utf16_and_utf32_positions() {
    let source = SourceFile::new("unicode", "a😀é");
    let index = source.line_index();

    assert_eq!(index.offset(0, 2, PositionEncoding::Utf16), Some(1));
    assert_eq!(index.offset(0, 3, PositionEncoding::Utf16), Some(5));
    assert_eq!(index.offset(0, 3, PositionEncoding::Utf8), Some(1));
    assert_eq!(index.offset(0, 2, PositionEncoding::Utf32), Some(5));
    assert_eq!(
        index.position(5, PositionEncoding::Utf16),
        Some(SourcePosition {
            line: 0,
            character: 3,
        })
    );
    assert_eq!(
        index
            .position(1, PositionEncoding::Utf16)
            .unwrap()
            .character,
        1
    );
    assert_eq!(
        index
            .position(2, PositionEncoding::Utf16)
            .unwrap()
            .character,
        1
    );
}

#[test]
fn line_index_handles_crlf_trailing_lines_and_clamped_positions() {
    let source = SourceFile::new("lines", "abc\r\ndef\n");
    let index = source.line_index();

    assert_eq!(index.offset(0, 20, PositionEncoding::Utf16), Some(3));
    assert_eq!(index.offset(2, 0, PositionEncoding::Utf16), Some(9));
    assert_eq!(index.offset(3, 0, PositionEncoding::Utf16), None);
    assert_eq!(
        index
            .position(4, PositionEncoding::Utf16)
            .unwrap()
            .character,
        3
    );
    assert_eq!(
        index.position(5, PositionEncoding::Utf16),
        Some(SourcePosition {
            line: 1,
            character: 0,
        })
    );
    assert_eq!(index.position(10, PositionEncoding::Utf16), None);
}

#[test]
fn source_locations_count_unicode_columns_and_lines() {
    let source = SourceFile::new("test", "aé\nb");

    assert_eq!(source.name(), "test");
    assert_eq!(source.text(), "aé\nb");
    assert_eq!(source.line_column(3), (1, 3));
    assert_eq!(source.line_column(4), (2, 1));
}

#[test]
fn database_keeps_identity_and_advances_revision_only_for_changed_text() {
    let mut sources = SourceDatabase::default();
    let id = sources.insert("main", "one").unwrap();
    assert_eq!(sources.revision(id), Some(1));
    assert_eq!(sources.update(id, "one").unwrap(), 1);
    assert_eq!(sources.update(id, "two").unwrap(), 2);
    assert_eq!(sources.id_by_name("main"), Some(id));
    let source = sources.get(id).unwrap();
    let identity = source.identity();
    assert!(source.is_database_managed());
    assert_eq!(identity.id(), id);
    assert_eq!(identity.revision(), 2);
}

#[test]
fn cloned_source_keeps_its_snapshot_after_database_update() {
    let mut sources = SourceDatabase::default();
    let id = sources.insert("main", "before").unwrap();
    let old_source = sources.get(id).unwrap().clone();

    sources.update(id, "after").unwrap();
    let current_source = sources.get(id).unwrap();

    assert_eq!(old_source.text(), "before");
    assert_eq!(old_source.identity().revision(), 1);
    assert_eq!(current_source.text(), "after");
    assert_eq!(current_source.identity().revision(), 2);
}

#[test]
fn source_identities_are_unique_across_database_instances_and_clones() {
    let mut original = SourceDatabase::default();
    let original_id = original.insert("main", "text").unwrap();
    let original_identity = original.get(original_id).unwrap().identity();

    let mut separate = SourceDatabase::default();
    let separate_id = separate.insert("main", "text").unwrap();
    let separate_identity = separate.get(separate_id).unwrap().identity();
    assert_ne!(original_identity, separate_identity);
    assert_ne!(
        original_identity.namespace_id(),
        separate_identity.namespace_id()
    );

    let cloned = original.clone();
    let cloned_identity = cloned.get(original_id).unwrap().identity();
    assert_ne!(original_identity, cloned_identity);
    assert_ne!(
        original_identity.namespace_id(),
        cloned_identity.namespace_id()
    );
    assert_eq!(cloned_identity.id(), original_id);
}

#[test]
fn standalone_sources_have_unique_identities_and_clones_keep_them() {
    let source = SourceFile::new("main.lang", "text");
    let clone = source.clone();
    let other_source = SourceFile::new("main.lang", "text");
    let mut database = SourceDatabase::default();
    let managed_id = database.insert("main.lang", "text").unwrap();
    let managed_source = database.get(managed_id).unwrap();

    assert!(!source.is_database_managed());
    assert!(!other_source.is_database_managed());
    assert!(managed_source.is_database_managed());
    assert_eq!(source.identity(), clone.identity());
    assert_ne!(source.identity(), other_source.identity());
    assert_ne!(source.identity(), managed_source.identity());
    assert_eq!(source.identity().revision(), 1);
}

#[test]
fn database_rejects_duplicate_names_and_unknown_ids() {
    let mut sources = SourceDatabase::default();
    let id = sources.insert("main", "").unwrap();
    assert_eq!(
        sources.insert("main", ""),
        Err(SourceDatabaseError::DuplicateName("main".into()))
    );
    sources.remove(id).unwrap();
    assert!(matches!(
        sources.update(id, ""),
        Err(SourceDatabaseError::UnknownSource(_))
    ));
    assert!(sources.get(id).is_none());
}

#[test]
fn span_length_is_saturating_for_invalid_ranges() {
    assert_eq!(Span::new(2, 5).len(), 3);
    assert_eq!(Span::new(5, 2).len(), 0);
}

#[test]
fn diagnostic_locations_retain_standalone_source_identity() {
    let source = SourceFile::new("language/constructs.langmeld", "construct");
    let diagnostic = Diagnostic::error("TEST-001", "invalid syntax", &source, Span::new(0, 9));

    let location = diagnostic.location.unwrap();
    assert_eq!(location.source_name(), "language/constructs.langmeld");
    assert_eq!(location.source_identity(), source.identity());
    assert_eq!(location.span(), Span::new(0, 9));
}
