use super::*;

#[test]
fn definition_lexical_rules_configure_comments_for_parsing_and_allow_definition_comments() {
    let source = SourceFile::new(
        "language.langmeld",
        r#"// Comments in definition files are recognized by the built-in meta-grammar.
        language "Toy" {
            lexical {
                line_comment "//";
                block_comment "/*" "*/";
            }
            construct "print" {
                syntax { literal "print"; string "value"; literal ";"; }
            }
        }"#,
    );

    let definition = parse_language_definition(&source);
    assert!(definition.is_valid(), "{:?}", definition.diagnostics);
    let language = definition.language.unwrap();
    assert_eq!(language.lexical_rules().line_comments(), ["//"]);
    assert_eq!(
        language.lexical_rules().block_comments(),
        [("/*".to_owned(), "*/".to_owned())]
    );

    let parsed = parse(
        &SourceFile::new("main.toy", "// before\nprint \"ok\"; /* after */"),
        &language,
    );
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert_eq!(parsed.constructs().len(), 1);
}

#[test]
fn invalid_definition_comment_rules_report_at_the_lexical_block() {
    let source = SourceFile::new(
        "language.langmeld",
        r##"language "Toy" {
            lexical {
                line_comment "#";
                block_comment "#" "*/";
            }
            construct "item" { syntax { literal "item"; } }
        }"##,
    );

    let definition = parse_language_definition(&source);

    assert!(definition.language.is_none());
    let diagnostic = definition
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "LM-DEF-004")
        .unwrap();
    let span = diagnostic.location.as_ref().unwrap().span();
    let text = &source.text()[span.start..span.end];
    assert!(text.contains("lexical"));
    assert!(text.contains("block_comment"));
}
