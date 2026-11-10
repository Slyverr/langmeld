use langmeld_core::{SourceDatabase, SourceDatabaseError, SourceFile, SourceIdentity, Span};

#[test]
fn source_locations_count_unicode_columns_and_lines() {
    let source = SourceFile::new("test", "aé\nb");
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
    assert_eq!(
        sources.get(id).unwrap().identity(),
        Some(SourceIdentity { id, revision: 2 })
    );
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
