use langmeld_core::{
    ConstructBuilder, FieldKind, FieldSpec, LanguageBuilder, ScopeLookup, ScopeSpec,
    SourceDatabase, SourceFile, Span, SymbolSpec,
};
use langmeld_language::{
    AnalysisLimits, AstNode, CapturedValue, PreparedLanguage, Program, Value, WorkspaceFile,
    analyze_workspace_files, analyze_workspace_with_limits,
};

fn language(scope_lookup: ScopeLookup) -> langmeld_core::ValidatedLanguageSpec {
    LanguageBuilder::new("AnalysisFixture")
        .version("0.1.0")
        .root("program")
        .construct(
            ConstructBuilder::new("program")
                .syntax_variant("root", "program {namespace} {")
                .field(FieldSpec::required("namespace", FieldKind::Identifier))
                .block(["program", "variable", "reference"]),
        )
        .construct(
            ConstructBuilder::new("variable")
                .syntax_variant("declaration", "let {name} = {value}")
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .field(FieldSpec::required("value", FieldKind::Number))
                .symbol(
                    SymbolSpec::new("variable", "name")
                        .with_scope(ScopeSpec::new("program", "namespace").lookup(scope_lookup)),
                ),
        )
        .construct(
            ConstructBuilder::new("reference")
                .syntax_variant("use", "use {target}")
                .field(FieldSpec::required(
                    "target",
                    FieldKind::reference("variable"),
                )),
        )
        .build()
        .expect("analysis fixture validates")
}

fn parse(spec: &langmeld_core::ValidatedLanguageSpec, source: &SourceFile) -> Program {
    PreparedLanguage::new(spec.clone())
        .expect("language patterns compile")
        .configure()
        .expect("fixture has no host callbacks")
        .parse(source)
        .program
}

#[test]
fn workspace_analysis_resolves_across_files_and_keeps_first_duplicate() {
    let spec = language(ScopeLookup::Exact);
    let declaration_source = SourceFile::new(
        "declarations.lang",
        "program main {\n  let item = 1\n  let item = 2\n}\n",
    );
    let reference_source = SourceFile::new(
        "references.lang",
        "program main {\n  use item\n  use absent\n}\n",
    );
    let declaration_program = parse(&spec, &declaration_source);
    let reference_program = parse(&spec, &reference_source);

    let analysis = analyze_workspace_with_limits(
        &spec,
        &[
            (&declaration_source, &declaration_program),
            (&reference_source, &reference_program),
        ],
        AnalysisLimits::default(),
    );

    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|item| { item.source_index == 0 && item.diagnostic.code == "LM2002" })
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|item| { item.source_index == 1 && item.diagnostic.code == "LM2001" })
    );
    let resolved = analysis
        .resolved_references
        .iter()
        .find(|reference| reference.name == "item")
        .expect("the cross-file reference resolves");
    assert_eq!(resolved.source_index, 1);
    assert_eq!(resolved.declaration_source_index, 0);
    assert_eq!(analysis.type_facts.len(), 8);
}

#[test]
fn lexical_parent_scope_lookup_resolves_from_nested_programs() {
    let spec = language(ScopeLookup::LexicalParents);
    let source = SourceFile::new(
        "nested.lang",
        "program outer {\n  let item = 1\n  program inner {\n    use item\n  }\n}\n",
    );
    let program = parse(&spec, &source);
    let analysis =
        analyze_workspace_with_limits(&spec, &[(&source, &program)], AnalysisLimits::default());

    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    assert_eq!(analysis.resolved_references.len(), 1);

    let exact = language(ScopeLookup::Exact);
    let exact_program = parse(&exact, &source);
    let exact_analysis = analyze_workspace_with_limits(
        &exact,
        &[(&source, &exact_program)],
        AnalysisLimits::default(),
    );
    assert!(
        exact_analysis
            .diagnostics
            .iter()
            .any(|item| { item.diagnostic.code == "LM2001" })
    );
}

#[test]
fn source_database_ids_and_revisions_are_attached_to_results() {
    let spec = language(ScopeLookup::Exact);
    let mut database = SourceDatabase::default();
    let declaration_id = database
        .insert("decl.lang", "program main {\n  let item = 1\n}\n")
        .expect("declaration source is indexed");
    let reference_id = database
        .insert("use.lang", "program main {\n  use item\n}\n")
        .expect("reference source is indexed");
    let declaration_source = database
        .get(declaration_id)
        .expect("source snapshot exists");
    let reference_source = database.get(reference_id).expect("source snapshot exists");
    let declaration_revision = database.revision(declaration_id).expect("revision exists");
    let reference_revision = database.revision(reference_id).expect("revision exists");
    let declaration_program = parse(&spec, declaration_source);
    let reference_program = parse(&spec, reference_source);
    let files = [
        WorkspaceFile {
            source_id: declaration_id,
            revision: declaration_revision,
            source: declaration_source,
            program: &declaration_program,
        },
        WorkspaceFile {
            source_id: reference_id,
            revision: reference_revision,
            source: reference_source,
            program: &reference_program,
        },
    ];

    let analysis = analyze_workspace_files(&spec, &files);
    let resolved = &analysis.resolved_references[0];
    assert_eq!(resolved.source_id, Some(reference_id));
    assert_eq!(resolved.source_revision, Some(1));
    assert_eq!(resolved.declaration_source_id, Some(declaration_id));
    assert_eq!(resolved.declaration_source_revision, Some(1));
    assert!(
        analysis
            .type_facts
            .iter()
            .any(|fact| fact.source_id == Some(declaration_id) && fact.source_revision == Some(1))
    );
}

#[test]
fn analysis_reports_type_mismatches_and_ignores_unknown_constructs_safely() {
    let spec = language(ScopeLookup::Exact);
    let source = SourceFile::new("manual.lang", "program main {}\n");
    let malformed_variable = AstNode::new("variable", "declaration", Span::new(15, 25))
        .with_field(
            "name",
            CapturedValue::new(Value::Identifier("item".to_owned()), Span::new(15, 19)),
        )
        .with_field(
            "value",
            CapturedValue::new(Value::Text("wrong".to_owned()), Span::new(22, 25)),
        );
    let program = Program {
        nodes: vec![
            AstNode::new("unknown_host_node", "host", Span::new(0, 0)),
            AstNode::new("program", "root", Span::new(0, source.text.len()))
                .with_field(
                    "namespace",
                    CapturedValue::new(Value::Identifier("main".to_owned()), Span::new(8, 12)),
                )
                .with_children([malformed_variable]),
        ],
    };

    let analysis =
        analyze_workspace_with_limits(&spec, &[(&source, &program)], AnalysisLimits::default());
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|item| item.diagnostic.code == "LM2010")
    );
}

#[test]
fn analysis_limits_bound_input_work_and_output() {
    let spec = language(ScopeLookup::Exact);
    let source = SourceFile::new(
        "limits.lang",
        "program main {\n  let item = 1\n  let item = 2\n  use missing\n}\n",
    );
    let program = parse(&spec, &source);
    let cases = [
        AnalysisLimits {
            max_files: 0,
            ..AnalysisLimits::default()
        },
        AnalysisLimits {
            max_work_items: 0,
            ..AnalysisLimits::default()
        },
        AnalysisLimits {
            max_nesting_depth: 0,
            ..AnalysisLimits::default()
        },
        AnalysisLimits {
            max_type_facts: 0,
            ..AnalysisLimits::default()
        },
        AnalysisLimits {
            max_resolved_references: 0,
            ..AnalysisLimits::default()
        },
    ];
    for limits in cases {
        let analysis = analyze_workspace_with_limits(&spec, &[(&source, &program)], limits);
        assert!(analysis.resource_limited, "{limits:?}");
        assert_eq!(analysis.diagnostics[0].diagnostic.code, "LM2090");
    }

    let analysis = analyze_workspace_with_limits(
        &spec,
        &[(&source, &program)],
        AnalysisLimits {
            max_diagnostics: 1,
            ..AnalysisLimits::default()
        },
    );
    assert!(analysis.resource_limited);
    assert_eq!(analysis.diagnostics.len(), 1);
    assert_eq!(analysis.diagnostics[0].diagnostic.code, "LM2090");
}
