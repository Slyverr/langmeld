use langmeld_core::{SourceFile, Span};
use langmeld_language::{LanguageAnalyzer, complete_with, definition_at, references_at};
use langmeld_lsp::{HostLanguageBackend, LanguageBackend};
use langmeld_schema_language_example::{
    SchemaAnalyzer, SchemaCompleter, SchemaNavigator, SchemaParser, SchemaWorkspaceAnalyzer,
    analyze, parse_schema,
};

#[test]
fn completes_field_types_from_the_current_schema_symbols() {
    let source = SourceFile::new(
        "schema.lang",
        "record User { manager: Man } record Manager {}",
    );
    let parsed = parse_schema(&source);
    let analysis = SchemaAnalyzer.analyze(&source, &parsed);

    let offset = source.text().find("Man").unwrap() + "Man".len();

    let candidates = complete_with(&source, &parsed, Some(&analysis), offset, &SchemaCompleter);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].text, "Manager");
    assert_eq!(candidates[0].replace, Span::new(offset - 3, offset));
}

#[test]
fn host_backend_connects_schema_analysis_completion_and_navigation() {
    let backend = HostLanguageBackend::new(SchemaParser)
        .with_analyzer(SchemaAnalyzer)
        .with_completer(SchemaCompleter)
        .with_navigator(SchemaNavigator);
    let source = SourceFile::new(
        "schema.lang",
        "record User { manager: Manager } record Manager {}",
    );
    let document = backend.analyze(&source);

    let type_use = source.text().find("Manager").unwrap();
    let completion_offset = type_use + "Man".len();
    let declaration = source.text().rfind("Manager").unwrap();

    assert!(backend.capabilities().completion);
    assert!(backend.capabilities().definition);
    assert!(backend.capabilities().references);
    assert!(document.diagnostics().is_empty());
    assert_eq!(
        backend.complete(&source, &document, completion_offset)[0].text,
        "Manager"
    );
    assert_eq!(
        backend.definition(&source, &document, type_use + 1)[0].span(),
        Span::new(declaration, declaration + "Manager".len())
    );
    assert_eq!(
        backend
            .references(&source, &document, type_use + 1, true)
            .len(),
        2
    );
}

#[test]
fn workspace_analysis_resolves_and_navigates_across_open_files() {
    let backend = HostLanguageBackend::new(SchemaParser)
        .with_analyzer(SchemaAnalyzer)
        .with_workspace_analyzer(SchemaWorkspaceAnalyzer)
        .with_completer(SchemaCompleter)
        .with_navigator(SchemaNavigator);
    let user = SourceFile::new("user.schema", "record User { manager: Manager }");
    let manager = SourceFile::new("manager.schema", "record Manager {}");
    let documents = backend
        .analyze_workspace(&[user.clone(), manager.clone()])
        .expect("workspace analyzer is configured");
    let type_offset = user.text().find("Manager").expect("type use");
    let declaration_offset = manager.text().find("Manager").expect("declaration");
    let completions = backend.complete(&user, &documents[0], type_offset + "Man".len());

    assert!(documents[0].diagnostics().is_empty());
    assert!(documents[1].diagnostics().is_empty());
    assert!(
        completions
            .iter()
            .any(|candidate| candidate.text == "Manager")
    );
    let definition = backend.definition(&user, &documents[0], type_offset + 1);
    assert_eq!(definition.len(), 1);
    assert_eq!(definition[0].source_name(), manager.name());
    assert_eq!(
        definition[0].span(),
        Span::new(declaration_offset, declaration_offset + 7)
    );

    let references = backend.references(&manager, &documents[1], declaration_offset + 1, true);
    assert_eq!(references.len(), 2);
    assert!(
        references
            .iter()
            .any(|location| location.source_name() == user.name())
    );
    assert!(
        references
            .iter()
            .any(|location| location.source_name() == manager.name())
    );
}

#[test]
fn navigates_forward_and_recursive_type_references() {
    let text = "record User { manager: Manager } record Manager { user: User }";
    let source = SourceFile::new("schema.lang", text);
    let parsed = parse_schema(&source);
    let analysis = SchemaAnalyzer.analyze(&source, &parsed);

    let use_offset = text.rfind("User").unwrap() + 1;
    let user_declaration = text.find("User").unwrap();

    let definition = definition_at(
        &source,
        &parsed,
        Some(&analysis),
        use_offset,
        &SchemaNavigator,
    );
    let references = references_at(
        &source,
        &parsed,
        Some(&analysis),
        user_declaration + 1,
        true,
        &SchemaNavigator,
    );

    assert_eq!(definition.len(), 1);
    assert_eq!(
        definition[0].span(),
        Span::new(user_declaration, user_declaration + 4)
    );
    assert_eq!(references.len(), 2);
    assert!(
        references
            .iter()
            .any(|location| location.span().start == user_declaration)
    );
    assert!(
        references
            .iter()
            .any(|location| location.span().start == text.rfind("User").unwrap())
    );
}

#[test]
fn resolves_forward_and_recursive_record_types() {
    let source = SourceFile::new(
        "schema.lang",
        "record User { manager: Manager, id: int } record Manager { user: User }",
    );
    let parsed = parse_schema(&source);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );

    let analysis = SchemaAnalyzer.analyze(&source, &parsed);

    assert!(analysis.is_valid(), "{:?}", analysis.diagnostics());
    assert_eq!(analysis.result().symbols.len(), 2);
    assert_eq!(analysis.result().references.len(), 2);
    assert!(
        analysis
            .result()
            .references
            .iter()
            .all(|reference| reference.target.is_some())
    );
}

#[test]
fn reports_duplicate_records_fields_and_unknown_types() {
    let source = SourceFile::new(
        "invalid.schema",
        "record User { id: int, id: string, owner: Missing } record User {}",
    );
    let parsed = parse_schema(&source);
    let analysis = analyze(&source, parsed.tree());

    assert!(
        analysis
            .diagnostics()
            .iter()
            .any(|d| d.code == "SCHEMA-SEM-001")
    );
    assert!(
        analysis
            .diagnostics()
            .iter()
            .any(|d| d.code == "SCHEMA-SEM-002")
    );
    assert!(
        analysis
            .diagnostics()
            .iter()
            .any(|d| d.code == "SCHEMA-SEM-003")
    );
    assert!(
        analysis
            .result()
            .references
            .iter()
            .any(|reference| { reference.name == "Missing" && reference.target.is_none() })
    );
}

#[test]
fn parser_recovers_to_later_record_declarations() {
    let source = SourceFile::new(
        "recovery.schema",
        "record Broken { field int } record Good { value: string }",
    );

    let parsed = parse_schema(&source);

    assert!(!parsed.diagnostics().is_empty());
    assert!(
        parsed
            .tree()
            .records
            .iter()
            .any(|record| record.name == "Good")
    );
}

#[test]
fn parser_preserves_the_next_record_after_a_missing_field_type() {
    let source = SourceFile::new(
        "recovery.schema",
        "record Broken { field: } record Good { value: string }",
    );

    let parsed = parse_schema(&source);

    assert!(!parsed.diagnostics().is_empty());
    assert!(
        parsed
            .tree()
            .records
            .iter()
            .any(|record| record.name == "Good")
    );
}

#[test]
fn analysis_retains_source_identity() {
    let mut database = langmeld_core::SourceDatabase::default();
    let id = database
        .insert("schema.lang", "record User { id: int }")
        .expect("insert source");
    let source = database.get(id).expect("stored source");
    let parsed = parse_schema(source);

    assert_eq!(
        analyze(source, parsed.tree()).source_identity(),
        parsed.source_identity()
    );
}
