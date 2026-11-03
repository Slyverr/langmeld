use langmeld_core::{
    BehaviorStage, ConstructBuilder, FieldKind, FieldSpec, LanguageBuilder, NamedRecord, SourceFile,
};
use langmeld_language::{
    DefinitionLimits, langmeld_definition_language, parse_definition, parse_definition_with_limits,
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
    let source = SourceFile::new("demo.lm", DEFINITION);
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
fn supplied_meta_language_uses_the_same_parser_and_conversion_path() {
    let source = SourceFile::new("demo.lm", DEFINITION);
    let meta = langmeld_definition_language().expect("the Rust bootstrap grammar validates");
    let converted = parse_language_definition_with_meta(&source, &meta)
        .expect("caller-supplied meta-language parses and converts");

    assert_eq!(converted.spec().name, "Demo");
    assert_eq!(converted.spec().constructs.len(), 2);
    assert_eq!(converted.spec().types[0].id, "Position");
}

#[test]
fn definition_limits_and_import_boundary_return_diagnostics() {
    let too_large = SourceFile::new("large.lm", DEFINITION);
    let diagnostics = parse_definition_with_limits(
        &too_large,
        DefinitionLimits {
            max_source_bytes: 1,
            ..DefinitionLimits::default()
        },
    )
    .expect_err("source limit is checked before parsing");
    assert_eq!(diagnostics[0].code, "LMDEF100");

    let diagnostics = parse_definition_with_limits(
        &too_large,
        DefinitionLimits {
            max_tokens: 1,
            ..DefinitionLimits::default()
        },
    )
    .expect_err("token limit is enforced by the generic parser");
    assert_eq!(diagnostics[0].code, "LMDEF101");

    let malformed = SourceFile::new("broken.lm", "language Broken {\n  version: \"0.1.0\"\n");
    let diagnostics = parse_definition(&malformed).expect_err("incomplete definitions fail");
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.span.is_some())
    );
    assert!(diagnostics[0].message.contains("broken.lm"));

    let imported = SourceFile::new(
        "imports.lm",
        "language Demo {\n  version: \"0.1.0\"\n  root: program\n  import: \"parts/types.lm\"\n}\n",
    );
    let diagnostics = parse_definition(&imported).expect_err("imports need a project loader");
    assert_eq!(diagnostics[0].code, "LMDEF045", "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("file-based loader"),
        "{diagnostics:?}"
    );
}
