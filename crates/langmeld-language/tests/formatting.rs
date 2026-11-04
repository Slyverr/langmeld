use langmeld_core::{
    BlockSeparator, BlockSeparatorPolicy, ConstructBuilder, FieldKind, FieldSpec, LanguageBuilder,
    SourceFile,
};
use langmeld_language::{FormatOptions, PreparedLanguage, format_parsed_source, format_source};

fn flow_language() -> langmeld_core::ValidatedLanguageSpec {
    LanguageBuilder::new("Flow")
        .version("0.1.0")
        .root("flow")
        .construct(
            ConstructBuilder::new("flow")
                .syntax("flow {name} {")
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .block(["node", "edge"]),
        )
        .construct(
            ConstructBuilder::new("node")
                .syntax("node {name}")
                .field(FieldSpec::required("name", FieldKind::Identifier)),
        )
        .construct(
            ConstructBuilder::new("edge")
                .syntax("{from} -> {to}")
                .field(FieldSpec::required("from", FieldKind::Identifier))
                .field(FieldSpec::required("to", FieldKind::Identifier)),
        )
        .build()
        .expect("flow test language validates")
}

fn text_language() -> langmeld_core::ValidatedLanguageSpec {
    LanguageBuilder::new("TextBlocks")
        .version("0.1.0")
        .root("program")
        .construct(
            ConstructBuilder::new("program")
                .syntax("program {")
                .block(["statement"]),
        )
        .construct(
            ConstructBuilder::new("statement")
                .syntax("text {value}")
                .field(FieldSpec::required("value", FieldKind::Text)),
        )
        .build()
        .expect("test language validates")
}

#[test]
fn formatter_preserves_strings_and_comments_and_is_idempotent() {
    let language = text_language();
    let source = SourceFile::new(
        "text-blocks.source",
        "program {\ntext \"brace } { and # text\" # comment } {\n}\n",
    );
    let formatted =
        format_source(&language, &source, FormatOptions::default()).expect("valid source formats");
    assert_eq!(
        formatted,
        "program {\n  text \"brace } { and # text\" # comment } {\n}\n"
    );

    let parsed = PreparedLanguage::new(language.clone())
        .expect("language prepares")
        .with_custom_parsers([])
        .expect("declarative language configures")
        .parse(&source);
    assert_eq!(
        format_parsed_source(&language, &source, &parsed, FormatOptions::default())
            .expect("existing parse result formats"),
        formatted
    );
    assert_eq!(
        format_source(
            &language,
            &SourceFile::new("text-blocks.source", &formatted),
            FormatOptions::default(),
        )
        .expect("formatted source remains valid"),
        formatted
    );
}

#[test]
fn formatter_normalizes_crlf_and_supports_tabs() {
    let language = flow_language();
    let source = SourceFile::new("flow.source", "flow Demo {\r\nnode api\r\n}\r\n");
    let formatted =
        format_source(&language, &source, FormatOptions::default()).expect("CRLF source formats");
    assert_eq!(formatted, "flow Demo {\n  node api\n}\n");

    let tabbed = format_source(
        &language,
        &source,
        FormatOptions {
            tab_size: 4,
            insert_spaces: false,
        },
    )
    .expect("tab indentation formats");
    assert_eq!(tabbed, "flow Demo {\n\tnode api\n}\n");
}

#[test]
fn formatter_handles_empty_input_and_rejects_invalid_indentation_options() {
    let language = flow_language();
    assert_eq!(
        format_source(
            &language,
            &SourceFile::new("empty.flow", ""),
            FormatOptions::default(),
        )
        .expect("empty input stays empty"),
        ""
    );

    let diagnostics = format_source(
        &language,
        &SourceFile::new("flow.source", "flow Demo {\nnode api\n}\n"),
        FormatOptions {
            tab_size: 0,
            insert_spaces: false,
        },
    )
    .expect_err("zero-size indentation is invalid");
    assert_eq!(diagnostics[0].code, "LMFMT001");
}

#[test]
fn formatter_rejects_invalid_programs_without_rewriting_them() {
    let source = SourceFile::new("flow.source", "flow Demo {\nnode\n}\n");
    let diagnostics = format_source(&flow_language(), &source, FormatOptions::default())
        .expect_err("malformed programs are not formatted");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == langmeld_core::Severity::Error)
    );
}

#[test]
fn formatter_uses_parser_owned_custom_block_delimiters() {
    let language = LanguageBuilder::new("ConfiguredBlocks")
        .version("0.1.0")
        .schema_version(2)
        .root("program")
        .construct(
            ConstructBuilder::new("program")
                .syntax("program")
                .block(["statement"])
                .block_delimiters("begin block", "end block")
                .block_separators(
                    [BlockSeparator::literal(";")],
                    BlockSeparatorPolicy::Required,
                ),
        )
        .construct(
            ConstructBuilder::new("statement")
                .syntax("step {name}")
                .field(FieldSpec::required("name", FieldKind::Identifier)),
        )
        .build()
        .expect("configured block language validates");
    let source = SourceFile::new(
        "configured.lang",
        "program begin block\nstep one;step two\nend block\n",
    );

    let parsed = PreparedLanguage::new(language.clone())
        .expect("language prepares")
        .configure()
        .expect("declarative parser configures")
        .parse(&source);
    assert_eq!(parsed.block_delimiters.len(), 2);
    assert!(parsed.block_delimiters[0].opens);
    assert!(!parsed.block_delimiters[1].opens);
    assert_eq!(
        format_parsed_source(&language, &source, &parsed, FormatOptions::default())
            .expect("custom block formats"),
        "program begin block\n  step one;step two\nend block\n"
    );
}
