use langmeld_core::SourceFile;
use langmeld_language::{DefinitionFormatOptions, format_language_definition};

#[test]
fn formats_nested_definition_blocks_and_is_idempotent() {
    let source = SourceFile::new(
        "language.langmeld",
        r#"language "Toy"{construct "item"{syntax{literal "item";identifier "name";}block "{" "}"{allow "item";}}}"#,
    );
    let formatted = format_language_definition(&source, DefinitionFormatOptions::default())
        .expect("valid definition should format");
    assert_eq!(
        formatted,
        r#"language "Toy" {
    construct "item" {
        syntax {
            literal "item";
            identifier "name";
        }
        block "{" "}" {
            allow "item";
        }
    }
}
"#
    );

    let formatted_source = SourceFile::new("language.langmeld", formatted);
    assert_eq!(
        format_language_definition(&formatted_source, DefinitionFormatOptions::default())
            .expect("formatted definition should remain valid"),
        formatted_source.text()
    );
}

#[test]
fn uses_editor_indentation_options() {
    let source = SourceFile::new(
        "language.langmeld",
        "language \"Toy\" { construct \"item\" { syntax { literal \"item\"; } } }",
    );
    let formatted = format_language_definition(
        &source,
        DefinitionFormatOptions {
            tab_size: 2,
            insert_spaces: false,
        },
    )
    .expect("valid definition should format");

    assert!(formatted.contains("\n\tconstruct \"item\" {"));
    assert!(formatted.contains("\n\t\tsyntax {"));
}

#[test]
fn refuses_to_rewrite_incomplete_definitions() {
    let source = SourceFile::new("language.langmeld", "language \"Toy\" {");

    let diagnostics = format_language_definition(&source, DefinitionFormatOptions::default())
        .expect_err("incomplete definition should not be rewritten");

    assert!(!diagnostics.is_empty());
}
