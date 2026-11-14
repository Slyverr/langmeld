use langmeld_core::SourceFile;
use langmeld_language::{parse, parse_language_definition};

#[test]
fn declarative_definitions_can_mark_a_construct_block_optional() {
    let source = SourceFile::new(
        "language/main.langmeld",
        r#"
language "Optional" {
    root "move";

    construct "move" {
        syntax {
            literal "move";
            identifier "target";
        }

        optional_block "{" "}" {
            allow "print";
        }
    }

    construct "print" {
        syntax {
            literal "print";
            identifier "value";
        }
    }
}
"#,
    );

    let definition = parse_language_definition(&source);
    assert!(
        definition.diagnostics.is_empty(),
        "{:?}",
        definition.diagnostics
    );
    let grammar = definition
        .language
        .expect("definition should produce a grammar");
    assert!(grammar.constructs()[0].block_spec().unwrap().is_optional());

    let parsed = parse(&SourceFile::new("main", "move item"), &grammar);
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.constructs()[0].block_span, None);
}
