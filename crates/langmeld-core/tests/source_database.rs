use langmeld_core::{SourceDatabase, SourceDatabaseError};

#[test]
fn source_identity_survives_text_updates_and_revisions_only_advance_on_change() {
    let mut database = SourceDatabase::default();
    let id = database
        .insert("project/main.lang", "first")
        .expect("source inserts");
    assert_eq!(database.revision(id), Some(1));
    assert_eq!(database.id_by_name("project/main.lang"), Some(id));

    assert_eq!(database.upsert("project/main.lang", "first"), Ok(id));
    assert_eq!(database.revision(id), Some(1));
    assert_eq!(database.upsert("project/main.lang", "second"), Ok(id));
    assert_eq!(database.revision(id), Some(2));
    assert_eq!(
        database.get(id).map(|source| source.text.as_str()),
        Some("second")
    );
}

#[test]
fn source_names_and_unknown_ids_are_checked() {
    let mut database = SourceDatabase::default();
    let id = database.insert("same.lang", "one").expect("source inserts");
    assert_eq!(
        database.insert("same.lang", "two"),
        Err(SourceDatabaseError::DuplicateName("same.lang".to_owned()))
    );
    assert_eq!(
        database.remove(id).map(|source| source.text),
        Some("one".to_owned())
    );
    assert_eq!(
        database.update(id, "text"),
        Err(SourceDatabaseError::UnknownSource(id))
    );
    assert!(database.is_empty());
    assert_eq!(database.id_by_name("same.lang"), None);
}

#[test]
fn indexed_line_columns_match_utf8_crlf_and_clamp_interior_offsets() {
    let mut database = SourceDatabase::default();
    let id = database
        .insert("unicode.lang", "a\r\n🦀z\n")
        .expect("source inserts");
    assert_eq!(database.line_column(id, 0), Some((1, 1)));
    assert_eq!(database.line_column(id, 2), Some((1, 3)));
    assert_eq!(database.line_column(id, 3), Some((2, 1)));
    assert_eq!(database.line_column(id, 4), Some((2, 1)));
    assert_eq!(database.line_column(id, 7), Some((2, 2)));
    assert_eq!(database.line_column(id, usize::MAX), Some((3, 1)));
}

#[test]
fn removal_and_reinsertion_allocate_a_new_identity() {
    let mut database = SourceDatabase::default();
    let first = database.insert("file.lang", "one").expect("source inserts");
    database.remove(first).expect("source removes");
    let second = database
        .insert("file.lang", "two")
        .expect("source reinserts");
    assert_ne!(first, second);
    assert_eq!(database.revision(second), Some(1));
}
