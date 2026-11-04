use langmeld_core::{
    BehaviorStage, BlockSeparator, BlockSeparatorPolicy, ConstructBuilder, FieldKind, FieldSpec,
    LanguageBuilder, NamedRecord, SourceFile,
};
use langmeld_language::{
    DefinitionLimits, PreparedLanguage, langmeld_definition_language, parse_definition,
    parse_definition_with_limits, parse_language_definition_modules,
    parse_language_definition_modules_with_limits, parse_language_definition_modules_with_meta,
    parse_language_definition_with_meta,
};

const DEFINITION: &str = r#"language Demo {
  version: "0.1.0"
  schema_version: 1
  root: program
  source_extension: ".demo"

  category statement {}
  rule word: "word"
  behavior analyze_demo: semantic_analysis

  enum Visibility {
    public
    private
  }

  type Position {
    field line required min 1 {
      scalar: number
    }
  }

  construct program {
    syntax {
      default: "program {namespace} {"
    }
    fields {
      namespace: identifier required
    }
    block: [declaration]
  }

  construct declaration {
    syntax {
      default: "let {name} {visibility}"
    }
    fields {
      name: identifier required
      visibility: Visibility required
      position: properties<Position> optional
    }
    implements: [statement]
  }
}
"#;

#[test]
fn built_in_frontend_converts_to_the_canonical_language_model() {
    let source = SourceFile::new("demo.langmeld", DEFINITION);
    let parsed = parse_definition(&source).expect("the built-in grammar converts the definition");
    let position = NamedRecord::new(
        "Position",
        [FieldSpec::required("line", FieldKind::Number).constraints(
            langmeld_core::FieldConstraints::numeric_range(Some(1), None),
        )],
    );
    let expected = LanguageBuilder::new("Demo")
        .version("0.1.0")
        .schema_version(1)
        .root("program")
        .source_extension(".demo")
        .category("statement")
        .rule("word", "word")
        .behavior("analyze_demo", BehaviorStage::SemanticAnalysis)
        .named_enum("Visibility", ["public", "private"])
        .record_type(position.clone())
        .construct(
            ConstructBuilder::new("program")
                .syntax_variant("default", "program {namespace} {")
                .field(FieldSpec::required("namespace", FieldKind::Identifier))
                .block(["declaration"]),
        )
        .construct(
            ConstructBuilder::new("declaration")
                .syntax_variant("default", "let {name} {visibility}")
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .field(FieldSpec::required(
                    "visibility",
                    FieldKind::named_enumeration("Visibility", ["public", "private"]),
                ))
                .field(FieldSpec::optional("position", position.field_kind()))
                .implements(["statement"]),
        )
        .build()
        .expect("the expected Rust model validates");

    assert_eq!(parsed, expected.spec().clone());
}

#[test]
fn definition_frontend_converts_schema_v2_block_layouts() {
    let source = SourceFile::new(
        "configured.langmeld",
        r#"language Configured {
  version: "0.1.0"
  schema_version: 2
  root: program

  construct program {
    syntax {
      default: "program {name}"
    }
    fields {
      name: identifier required
    }
    block: [statement] {
      open: "begin"
      close: "end"
      separator: ";"
      separator_policy: required
    }
  }

  construct statement {
    syntax {
      default: "step {name}"
    }
    fields {
      name: identifier required
    }
  }
}
"#,
    );
    let parsed = parse_definition(&source).expect("configured block declaration converts");
    let program = parsed
        .constructs
        .iter()
        .find(|construct| construct.id == "program")
        .expect("program construct is converted");
    let block = program.block.as_ref().unwrap();

    assert_eq!(parsed.schema_version, 2);
    assert_eq!(program.syntax, ["program {name}"]);
    assert_eq!(block.open, "begin");
    assert_eq!(block.close, "end");
    assert_eq!(block.separators, [BlockSeparator::literal(";")]);
    assert_eq!(block.separator_policy, BlockSeparatorPolicy::Required);

    let language = parsed
        .validate()
        .expect("converted schema-v2 model validates");
    let parser = PreparedLanguage::new(language)
        .expect("converted schema-v2 patterns compile")
        .configure()
        .expect("definition declares no host callbacks");
    let source = SourceFile::new(
        "configured.source",
        "program demo begin\nstep first;step second\nend\n",
    );
    let parsed_source = parser.parse(&source);
    assert!(
        parsed_source.diagnostics.is_empty(),
        "{:?}",
        parsed_source.diagnostics
    );
    assert_eq!(parsed_source.program.nodes[0].children.len(), 2);
    assert_eq!(
        parsed_source.program.nodes[0].children[1].fields["name"].value,
        langmeld_language::Value::Identifier("second".to_owned())
    );
}

#[test]
fn definition_frontend_reports_duplicate_block_layout_declarations_at_source() {
    let source = SourceFile::new(
        "duplicate.langmeld",
        r#"language Duplicate {
  version: "0.1.0"
  schema_version: 2
  root: program
  construct program {
    syntax { default: "program" }
    block: [] {
      open: "begin"
      open: "start"
      close: "end"
    }
  }
}
"#,
    );

    let diagnostics =
        parse_definition(&source).expect_err("duplicate block declarations are rejected");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("block opener is declared more than once")
            && diagnostic.span.is_some_and(|span| span.start < span.end)
    }));
}

#[test]
fn supplied_meta_language_uses_the_same_parser_and_conversion_path() {
    let source = SourceFile::new("demo.langmeld", DEFINITION);
    let meta = langmeld_definition_language().expect("the Rust bootstrap grammar validates");
    let converted = parse_language_definition_with_meta(&source, &meta)
        .expect("caller-supplied meta-language parses and converts");

    assert_eq!(converted.spec().name, "Demo");
    assert_eq!(converted.spec().constructs.len(), 2);
    assert_eq!(converted.spec().types[0].id, "Position");
}

#[test]
fn in_memory_modules_compose_without_filesystem_access() {
    let root = SourceFile::new(
        "language/language.langmeld",
        "language MemoryLanguage {\n  version: \"0.1.0\"\n  root: program\n  source_extension: \".mem\"\n}\n",
    );
    let modules = [SourceFile::new(
        "language/constructs/program.langmeld",
        "constructs {\n  construct program {\n    syntax {\n      default: \"program {\"\n    }\n    block: []\n  }\n}\n",
    )];

    let built_in = parse_language_definition_modules(&root, &modules)
        .expect("built-in grammar composes supplied sources in memory");
    let meta = langmeld_definition_language().expect("bootstrap grammar validates");
    let supplied = parse_language_definition_modules_with_meta(&root, &modules, &meta)
        .expect("caller-supplied grammar composes the same sources");

    assert_eq!(built_in.spec(), supplied.spec());
    assert_eq!(built_in.spec().name, "MemoryLanguage");
    assert!(built_in.construct("program").is_some());
}

#[test]
fn definition_limits_and_import_boundary_return_diagnostics() {
    let too_large = SourceFile::new("large.langmeld", DEFINITION);
    let diagnostics = parse_definition_with_limits(
        &too_large,
        DefinitionLimits {
            max_source_bytes: 1,
            ..DefinitionLimits::default()
        },
    )
    .expect_err("source limit is checked before parsing");
    assert_eq!(diagnostics[0].code, "LMDEF100");

    let root = SourceFile::new(
        "language.langmeld",
        "language MemoryLanguage { version: \"0.1.0\" root: program }",
    );
    let module = SourceFile::new("constructs/program.langmeld", "constructs { } ");
    let diagnostics = parse_language_definition_modules_with_limits(
        &root,
        std::slice::from_ref(&module),
        DefinitionLimits {
            max_source_bytes: root.text.len(),
            ..DefinitionLimits::default()
        },
    )
    .expect_err("the aggregate module size is bounded");
    assert_eq!(diagnostics[0].code, "LMDEF100");

    let diagnostics = parse_language_definition_modules_with_limits(
        &root,
        std::slice::from_ref(&module),
        DefinitionLimits {
            max_files: 1,
            ..DefinitionLimits::default()
        },
    )
    .expect_err("the in-memory project file count is bounded");
    assert_eq!(diagnostics[0].code, "LMDEF103");

    let diagnostics = parse_definition_with_limits(
        &too_large,
        DefinitionLimits {
            max_tokens: 1,
            ..DefinitionLimits::default()
        },
    )
    .expect_err("token limit is enforced by the generic parser");
    assert_eq!(diagnostics[0].code, "LMDEF101");

    let malformed = SourceFile::new(
        "broken.langmeld",
        "language Broken {\n  version: \"0.1.0\"\n",
    );
    let diagnostics = parse_definition(&malformed).expect_err("incomplete definitions fail");
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.span.is_some())
    );
    assert!(diagnostics[0].message.contains("broken.langmeld"));

    let malformed = SourceFile::new(
        "noisy.langmeld",
        "language Broken {\n  invalid one\n  invalid two\n  invalid three\n}\n",
    );
    let diagnostics = parse_definition_with_limits(
        &malformed,
        DefinitionLimits {
            max_diagnostics: 1,
            ..DefinitionLimits::default()
        },
    )
    .expect_err("malformed source is rejected under the diagnostic limit");
    assert!(diagnostics.len() <= 1, "{diagnostics:?}");

    let imported = SourceFile::new(
        "imports.langmeld",
        "language Demo {\n  version: \"0.1.0\"\n  root: program\n  import: \"parts/types.langmeld\"\n}\n",
    );
    let diagnostics = parse_definition(&imported).expect_err("imports need a project loader");
    assert_eq!(diagnostics[0].code, "LMDEF045", "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("file-based loader"),
        "{diagnostics:?}"
    );
}
