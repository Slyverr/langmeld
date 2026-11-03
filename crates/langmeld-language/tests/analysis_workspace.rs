use langmeld_core::{
    BehaviorStage, ConstructBuilder, FieldKind, FieldSpec, LanguageBuilder, ScopeLookup, ScopeSpec,
    SourceDatabase, SourceFile, Span, SymbolSpec,
};
use langmeld_language::{
    AnalysisLimits, AstNode, BehaviorRegistrationIssue, CapturedValue,
    ConfiguredSemanticHookRegistration, ConfiguredSemanticHookRegistry, HostTypeFact,
    PreparedLanguage, Program, ResolvedReference, SemanticHookRegistration, SemanticHookRegistry,
    Value, WorkspaceFile, WorkspaceSymbolResolverHook, WorkspaceSymbolResolverRegistration,
    WorkspaceSymbolResolverRegistry, WorkspaceTypeAnalysisHook, WorkspaceTypeAnalysisRegistration,
    WorkspaceTypeAnalysisRegistry, analyze_workspace_files,
    analyze_workspace_files_with_host_callbacks, analyze_workspace_with_host_callbacks_and_limits,
    analyze_workspace_with_limits,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
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

#[test]
fn host_callbacks_replace_resolution_and_preserve_indexed_facts() {
    let spec = language(ScopeLookup::Exact);
    let source = SourceFile::new("host.lang", "program main {\n  use absent\n}\n");
    let program = parse(&spec, &source);
    let mut database = SourceDatabase::default();
    let source_id = database
        .insert(source.name.clone(), source.text.clone())
        .expect("source registers");
    let revision = database.revision(source_id).expect("revision exists");
    let files = [WorkspaceFile {
        source_id,
        revision,
        source: &source,
        program: &program,
    }];
    let resolver: WorkspaceSymbolResolverHook = Box::new(|_, files, analysis| {
        let span = files[0].0.text.find("absent").expect("reference exists");
        let declaration = files[0].0.text.find("main").expect("declaration exists");
        analysis.resolved_references.push(ResolvedReference {
            source_index: 0,
            source_id: None,
            source_revision: None,
            reference_span: Span::new(span, span + "absent".len()),
            declaration_source_index: 0,
            declaration_source_id: None,
            declaration_source_revision: None,
            declaration_span: Span::new(declaration, declaration + "main".len()),
            kind: "variable".to_owned(),
            name: "absent".to_owned(),
        });
    });
    let resolver_hooks = [resolver];
    let semantic_hooks = [host_semantic_hook as langmeld_language::SemanticHook];
    let type_hooks: [WorkspaceTypeAnalysisHook; 1] = [Box::new(
        |_, files, generic_facts, resolved, host_facts, diagnostics| {
            assert!(!generic_facts.is_empty());
            assert_eq!(resolved.len(), 1);
            host_facts.push(HostTypeFact::new(0, Span::new(0, 7), "Program"));
            diagnostics.push(langmeld_language::WorkspaceDiagnostic {
                source_index: 0,
                source_id: None,
                source_revision: None,
                diagnostic: langmeld_core::Diagnostic::error(
                    "HOST2000",
                    "workspace callback ran",
                    Span::new(0, files[0].0.text.len().min(7)),
                ),
            });
        },
    )];

    let analysis = analyze_workspace_files_with_host_callbacks(
        &spec,
        &files,
        &semantic_hooks,
        &[],
        &resolver_hooks,
        &type_hooks,
    );

    assert!(!analysis.resource_limited);
    assert!(
        !analysis
            .diagnostics
            .iter()
            .any(|item| item.diagnostic.code == "LM2001")
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|item| item.diagnostic.code == "HOST1000")
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|item| item.diagnostic.code == "HOST2000")
    );
    assert_eq!(analysis.resolved_references[0].name, "absent");
    assert_eq!(analysis.resolved_references[0].source_id, Some(source_id));
    assert_eq!(
        analysis.resolved_references[0].declaration_source_id,
        Some(source_id)
    );
    assert_eq!(analysis.host_type_facts[0].source_id, Some(source_id));
    assert_eq!(analysis.host_type_facts[0].source_revision, Some(revision));
}

#[test]
fn invalid_host_spans_are_dropped_and_reported() {
    let spec = language(ScopeLookup::Exact);
    let source = SourceFile::new("invalid-host.lang", "program main {}\n");
    let program = parse(&spec, &source);
    let hooks: [WorkspaceTypeAnalysisHook; 1] = [Box::new(|_, _, _, _, facts, diagnostics| {
        facts.push(HostTypeFact::new(5, Span::new(0, 1), "OutOfRangeFile"));
        facts.push(HostTypeFact::new(
            0,
            Span::new(0, usize::MAX),
            "OutOfRangeSpan",
        ));
        diagnostics.push(langmeld_language::WorkspaceDiagnostic {
            source_index: 0,
            source_id: None,
            source_revision: None,
            diagnostic: langmeld_core::Diagnostic::error(
                "BADSPAN",
                "invalid diagnostic span",
                Span::new(1, usize::MAX),
            ),
        });
    })];

    let analysis = analyze_workspace_with_host_callbacks_and_limits(
        &spec,
        &[(&source, &program)],
        &[],
        &[],
        &[],
        &hooks,
        AnalysisLimits::default(),
    );

    assert!(analysis.host_type_facts.is_empty());
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|item| item.diagnostic.code == "LM2091")
    );
    assert!(
        !analysis
            .diagnostics
            .iter()
            .any(|item| item.diagnostic.code == "BADSPAN")
    );
}

#[test]
fn type_callbacks_skip_limited_inputs_and_share_the_fact_output_cap() {
    let spec = language(ScopeLookup::Exact);
    let source = SourceFile::new("bounded-host.lang", "program main {}\n");
    let program = parse(&spec, &source);
    let calls = Arc::new(AtomicUsize::new(0));
    let observed_calls = Arc::clone(&calls);
    let hooks: [WorkspaceTypeAnalysisHook; 1] = [Box::new(move |_, _, _, _, facts, _| {
        observed_calls.fetch_add(1, Ordering::Relaxed);
        facts.push(HostTypeFact::new(0, Span::new(0, 7), "HostTypeA"));
        facts.push(HostTypeFact::new(0, Span::new(0, 7), "HostTypeB"));
    })];

    let limited = analyze_workspace_with_host_callbacks_and_limits(
        &spec,
        &[(&source, &program)],
        &[],
        &[],
        &[],
        &hooks,
        AnalysisLimits {
            max_work_items: 0,
            ..AnalysisLimits::default()
        },
    );
    assert!(limited.resource_limited);
    assert_eq!(calls.load(Ordering::Relaxed), 0);

    let generic =
        analyze_workspace_with_limits(&spec, &[(&source, &program)], AnalysisLimits::default());
    let capped = analyze_workspace_with_host_callbacks_and_limits(
        &spec,
        &[(&source, &program)],
        &[],
        &[],
        &[],
        &hooks,
        AnalysisLimits {
            max_type_facts: generic.type_facts.len() + 1,
            ..AnalysisLimits::default()
        },
    );
    assert!(capped.resource_limited);
    assert_eq!(capped.host_type_facts.len(), 1);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn behavior_registry_reports_sorted_missing_duplicate_and_undeclared_ids() {
    let spec = LanguageBuilder::new("BehaviorFixture")
        .version("0.1.0")
        .root("program")
        .behavior("needed", BehaviorStage::SemanticAnalysis)
        .behavior("missing", BehaviorStage::SemanticAnalysis)
        .behavior("resolve", BehaviorStage::SymbolResolution)
        .construct(
            ConstructBuilder::new("program")
                .syntax_variant("root", "program {")
                .block(["program"]),
        )
        .build()
        .expect("behavior fixture validates");
    let result = SemanticHookRegistry::new(
        &spec,
        [
            SemanticHookRegistration::new("needed", no_op_semantic_hook),
            SemanticHookRegistration::new("needed", no_op_semantic_hook),
            SemanticHookRegistration::new("extra", no_op_semantic_hook),
            SemanticHookRegistration::new("resolve", no_op_semantic_hook),
        ],
    );
    let error = match result {
        Ok(_) => panic!("incomplete registrations are rejected"),
        Err(error) => error,
    };

    assert_eq!(
        error.issues,
        vec![
            BehaviorRegistrationIssue::Duplicate("needed".to_owned()),
            BehaviorRegistrationIssue::Missing("missing".to_owned()),
            BehaviorRegistrationIssue::Undeclared("extra".to_owned()),
            BehaviorRegistrationIssue::Undeclared("resolve".to_owned()),
        ]
    );
}

#[test]
fn each_behavior_stage_accepts_its_matching_callback_form() {
    let spec = LanguageBuilder::new("BehaviorFixture")
        .version("0.1.0")
        .root("program")
        .behavior("check", BehaviorStage::SemanticAnalysis)
        .behavior("resolve", BehaviorStage::SymbolResolution)
        .behavior("infer", BehaviorStage::WorkspaceTypeAnalysis)
        .construct(
            ConstructBuilder::new("program")
                .syntax_variant("root", "program {")
                .block(["program"]),
        )
        .build()
        .expect("behavior fixture validates");

    let configured = ConfiguredSemanticHookRegistry::new(
        &spec,
        [ConfiguredSemanticHookRegistration::new(
            "check",
            |_, _, _| {},
        )],
    );
    let resolver = WorkspaceSymbolResolverRegistry::new(
        &spec,
        [WorkspaceSymbolResolverRegistration::new(
            "resolve",
            Box::new(|_, _, _| {}),
        )],
    );
    let type_analysis = WorkspaceTypeAnalysisRegistry::new(
        &spec,
        [WorkspaceTypeAnalysisRegistration::new(
            "infer",
            Box::new(|_, _, _, _, _, _| {}),
        )],
    );

    assert_eq!(
        configured
            .expect("configured semantic callback matches")
            .hooks()
            .len(),
        1
    );
    assert_eq!(
        resolver.expect("resolver callback matches").hooks().len(),
        1
    );
    assert_eq!(
        type_analysis.expect("type callback matches").hooks().len(),
        1
    );
}

fn no_op_semantic_hook(_: &Program, _: &SourceFile, _: &mut Vec<langmeld_core::Diagnostic>) {}

fn host_semantic_hook(
    _: &Program,
    source: &SourceFile,
    diagnostics: &mut Vec<langmeld_core::Diagnostic>,
) {
    let start = source.text.find("program").expect("root exists");
    diagnostics.push(langmeld_core::Diagnostic::error(
        "HOST1000",
        "host semantic finding",
        Span::new(start, start + "program".len()),
    ));
}
