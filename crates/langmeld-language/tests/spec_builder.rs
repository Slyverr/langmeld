use langmeld_core::SourceFile;
use langmeld_language::{
    ConstructSpec, LanguageSpec, PatternPart, SpecError, parse, parse_language_definition,
};

fn declarative_language() -> LanguageSpec {
    let definition = parse_language_definition(&SourceFile::new(
        "language.langmeld",
        r##"
language "Mixed" {
    root "program";
    lexical {
        line_comment "#";
    }
    construct "program" {
        syntax {
            literal "program";
        }
        block "{" "}" {
            allow_category "statement";
        }
    }
    construct "declarative" {
        syntax {
            literal "decl";
            literal ";";
        }
        implements "statement";
    }
}
"##,
    ));

    assert!(definition.diagnostics.is_empty());
    definition
        .language
        .expect("definition should produce a language")
}

fn rust_statement() -> ConstructSpec {
    ConstructSpec::new(
        "host_statement",
        [PatternPart::literal("host"), PatternPart::literal(";")],
    )
    .implements(&["statement"])
}

#[test]
fn adds_rust_constructs_to_a_declarative_language() {
    let language = declarative_language()
        .into_builder()
        .construct(rust_statement())
        .build()
        .expect("combined declarations should validate");

    assert_eq!(language.lexical_rules().line_comments(), ["#"]);
    assert_eq!(
        language
            .root_constructs()
            .map(|construct| construct.name())
            .collect::<Vec<_>>(),
        ["program"]
    );

    let source = SourceFile::new("main.mixed", "program { decl; host; }");
    let parsed = parse(&source, &language);

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.constructs()[0].children.len(), 2);
    assert_eq!(parsed.constructs()[0].children[0].name, "declarative");
    assert_eq!(parsed.constructs()[0].children[1].name, "host_statement");
}

#[test]
fn allows_rust_constructs_at_the_root_when_requested() {
    let language = declarative_language()
        .into_builder()
        .construct(rust_statement())
        .with_root_constructs(&["program", "host_statement"])
        .build()
        .expect("the root allowlist should accept the added construct");
    let source = SourceFile::new("main.mixed", "host;");
    let parsed = parse(&source, &language);

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.constructs()[0].name, "host_statement");
}

#[test]
fn builder_validates_the_complete_rust_grammar_and_reports_conflicts() {
    let language = LanguageSpec::builder("Rust language")
        .construct(
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["statement"],
            ),
        )
        .extend([ConstructSpec::new(
            "statement",
            [PatternPart::literal("statement")],
        )])
        .build()
        .expect("the later child construct should satisfy the block");

    assert_eq!(language.constructs().len(), 2);

    let source = SourceFile::new("main.rustlang", "program { statement }");
    let parsed = parse(&source, &language);

    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.constructs()[0].name, "program");
    assert_eq!(parsed.constructs()[0].children[0].name, "statement");

    let error = declarative_language()
        .into_builder()
        .construct(ConstructSpec::new(
            "declarative",
            [PatternPart::literal("duplicate")],
        ))
        .build()
        .expect_err("duplicate construct names must be rejected");

    assert!(matches!(error, SpecError::DuplicateConstruct(name) if name == "declarative"));
}
