use langmeld_core::{
    BlockSeparator, BlockSeparatorPolicy, ConstructBuilder, Diagnostic, FieldKind, FieldSpec,
    LanguageBuilder, SourceFile, Span,
};
use langmeld_language::{
    AstNode, CapturedValue, CustomConstructMatch, CustomConstructParser, CustomLexer,
    LanguageToken, LanguageTokenKind, LexedSource, ParseLimits, PreparedLanguage, SyntaxTokenKind,
    Value, classify_parse_result_prepared,
};

fn language() -> PreparedLanguage {
    let spec = LanguageBuilder::new("ParserTests")
        .version("0.1.0")
        .root("program")
        .construct(
            ConstructBuilder::new("program")
                .syntax("program {")
                .block(["binding", "custom"]),
        )
        .construct(
            ConstructBuilder::new("binding")
                .syntax("let {name} = {value}")
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .field(FieldSpec::required("value", FieldKind::Number)),
        )
        .construct(
            ConstructBuilder::new("custom")
                .syntax("CUSTOM {name}")
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .custom_parser("host-special"),
        )
        .build()
        .expect("parser test language is valid");
    PreparedLanguage::new(spec).expect("all syntax patterns compile")
}

fn completion_language() -> PreparedLanguage {
    let spec = LanguageBuilder::new("CompletionTests")
        .version("0.1.0")
        .root("program")
        .construct(
            ConstructBuilder::new("program")
                .syntax("program {")
                .block(["paint"]),
        )
        .construct(
            ConstructBuilder::new("paint")
                .syntax("paint {color}")
                .field(FieldSpec::required(
                    "color",
                    FieldKind::Enum {
                        values: vec!["red".to_owned(), "green".to_owned()],
                    },
                )),
        )
        .build()
        .expect("completion language is valid");
    PreparedLanguage::new(spec).expect("completion syntax compiles")
}

fn nested_block_parser() -> langmeld_language::ConfiguredLanguageParser {
    let spec = LanguageBuilder::new("NestedBlocks")
        .version("0.1.0")
        .root("block")
        .construct(
            ConstructBuilder::new("block")
                .syntax("block {")
                .block(["block", "binding"]),
        )
        .construct(
            ConstructBuilder::new("binding")
                .syntax("let {name} = {value}")
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .field(FieldSpec::required("value", FieldKind::Number)),
        )
        .build()
        .expect("nested block language is valid");
    PreparedLanguage::new(spec)
        .expect("nested block patterns compile")
        .configure()
        .expect("no callback registrations are required")
}

fn configured_delimiter_parser(
    separator_policy: BlockSeparatorPolicy,
) -> langmeld_language::ConfiguredLanguageParser {
    let spec = LanguageBuilder::new("ConfiguredBlocks")
        .schema_version(2)
        .version("0.1.0")
        .root("module")
        .construct(
            ConstructBuilder::new("module")
                .syntax("module {name}")
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .block(["binding"])
                .block_delimiters("begin", "end")
                .block_separators([BlockSeparator::literal(";")], separator_policy),
        )
        .construct(
            ConstructBuilder::new("binding")
                .syntax("let {name} = {value}")
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .field(FieldSpec::required("value", FieldKind::Number)),
        )
        .build()
        .expect("configured block language is valid");
    PreparedLanguage::new(spec)
        .expect("configured block patterns compile")
        .configure()
        .expect("no callback registrations are required")
}

#[test]
fn parser_retains_expected_capture_and_block_contexts_for_completion() {
    let parser = completion_language()
        .with_custom_parsers([])
        .expect("no custom parsers are required");
    let source = SourceFile::new("completion.lang", "program {\npaint gr\n}\n");
    let parsed = parser.parse(&source);
    let frontier = parsed
        .completion_frontiers
        .iter()
        .find(|frontier| frontier.span == Span::new(16, 18))
        .expect("incomplete enum capture has a completion frontier");
    assert!(!frontier.ambiguous);
    assert_eq!(frontier.captures.len(), 1);
    assert_eq!(frontier.captures[0].construct, "paint");
    assert_eq!(frontier.captures[0].field, "color");
    assert_eq!(
        frontier.captures[0].kind,
        FieldKind::Enum {
            values: vec!["red".to_owned(), "green".to_owned()]
        }
    );
    assert!(parsed.completion_blocks.iter().any(|context| {
        context.parent == "program"
            && context.allowed == ["paint"]
            && context.span.start <= 16
            && context.span.end >= 18
    }));
}

#[test]
fn parser_exposes_safe_compiled_suffix_for_incomplete_patterns() {
    use langmeld_language::CompletionPart;

    let spec = LanguageBuilder::new("FunctionCompletions")
        .version("0.1.0")
        .root("program")
        .construct(
            ConstructBuilder::new("program")
                .syntax("program {")
                .block(["function"]),
        )
        .construct(
            ConstructBuilder::new("function")
                .syntax(r"fn {name}\({parameters}\) -> {result}")
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .field(FieldSpec::required("parameters", FieldKind::Identifier))
                .field(FieldSpec::required("result", FieldKind::Identifier)),
        )
        .build()
        .expect("function completion language is valid");
    let parser = PreparedLanguage::new(spec)
        .expect("function syntax compiles")
        .with_custom_parsers([])
        .expect("no custom parsers are required");
    let source = SourceFile::new("function.lang", "program {\nfn main\n}\n");
    let parsed = parser.parse(&source);
    let frontier = parsed
        .completion_frontiers
        .iter()
        .find(|frontier| frontier.continuation.is_some())
        .expect("the incomplete declaration has a safe completion frontier");
    assert_eq!(
        frontier.continuation,
        Some(vec![
            CompletionPart::Literal("(".to_owned()),
            CompletionPart::Capture("parameters".to_owned()),
            CompletionPart::Literal(")".to_owned()),
            CompletionPart::Literal("->".to_owned()),
            CompletionPart::Capture("result".to_owned()),
        ])
    );
}

fn configured() -> langmeld_language::ConfiguredLanguageParser {
    language()
        .with_custom_parsers([CustomConstructParser::new("host-special", |_, _| Ok(None))])
        .expect("custom parser is registered")
}

#[test]
fn parses_nested_constructs_and_retains_typed_fields_and_spans() {
    let result = configured().parse(&SourceFile::new(
        "main.lang",
        "program {\nlet count = 2\nlet total = 3.5\n}\n",
    ));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let root = &result.program.nodes[0];
    assert_eq!(root.construct, "program");
    assert_eq!(root.children.len(), 2);
    assert!(
        matches!(root.children[0].fields["name"].value, Value::Identifier(ref name) if name == "count")
    );
    assert!(matches!(
        root.children[0].fields["value"].value,
        Value::Number(2.0)
    ));
    assert_eq!(root.children[0].fields["name"].span, Span::new(14, 19));
    assert_eq!(root.span, Span::new(0, 41));
}

#[test]
fn block_children_accept_newline_and_semicolon_separators_by_default() {
    let result = configured().parse(&SourceFile::new(
        "separators.lang",
        "program {\nlet first = 1; let second = 2;\n}\n",
    ));

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let children = &result.program.nodes[0].children;
    assert_eq!(children.len(), 2);
    assert_eq!(
        children[0].fields["name"].value,
        Value::Identifier("first".to_owned())
    );
    assert_eq!(
        children[1].fields["name"].value,
        Value::Identifier("second".to_owned())
    );
}

#[test]
fn nested_blocks_balance_their_own_braces() {
    let result = nested_block_parser().parse(&SourceFile::new(
        "nested-blocks.lang",
        "block {\nblock {\nlet inner = 1\n}\nlet outer = 2\n}\n",
    ));

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let root = &result.program.nodes[0];
    assert_eq!(root.children.len(), 2);
    assert_eq!(root.children[0].construct, "block");
    assert_eq!(root.children[0].children.len(), 1);
    assert_eq!(root.children[1].construct, "binding");
}

#[test]
fn missing_nested_block_closers_preserve_partial_tree_and_report_each_scope() {
    let source = SourceFile::new("nested-open.lang", "block {\nblock {\nlet inner = 1\n");
    let result = nested_block_parser().parse(&source);

    let missing_closers = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "LM1204")
        .collect::<Vec<_>>();
    assert_eq!(missing_closers.len(), 2, "{:?}", result.diagnostics);
    assert!(
        missing_closers
            .iter()
            .all(|diagnostic| diagnostic.span
                == Some(Span::new(source.text.len(), source.text.len())))
    );

    let outer = &result.program.nodes[0];
    let inner = &outer.children[0];
    assert_eq!(outer.construct, "block");
    assert_eq!(inner.construct, "block");
    assert_eq!(inner.children.len(), 1);
    assert_eq!(inner.children[0].construct, "binding");
    assert_eq!(inner.span.end, source.text.len());
    assert_eq!(outer.span.end, source.text.len());
}

#[test]
fn configured_block_delimiters_and_required_separators_drive_parsing() {
    let parser = configured_delimiter_parser(BlockSeparatorPolicy::Required);
    let result = parser.parse(&SourceFile::new(
        "configured-blocks.lang",
        "module Main begin\nlet first = 1;\nlet second = 2\nend\n",
    ));

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let module = &result.program.nodes[0];
    assert_eq!(
        module.fields["name"].value,
        Value::Identifier("Main".to_owned())
    );
    assert_eq!(module.children.len(), 2);
    assert_eq!(
        module.span.end,
        "module Main begin\nlet first = 1;\nlet second = 2\nend".len()
    );
}

#[test]
fn required_block_separators_report_missing_boundaries_and_keep_children() {
    let result =
        configured_delimiter_parser(BlockSeparatorPolicy::Required).parse(&SourceFile::new(
            "missing-separator.lang",
            "module Main begin\nlet first = 1\nlet second = 2\nend\n",
        ));

    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM1206")
    );
    assert_eq!(result.program.nodes[0].children.len(), 2);
}

#[test]
fn child_errors_recover_and_keep_valid_siblings() {
    let result = configured().parse(&SourceFile::new(
        "recovery.lang",
        "program {\nlet first = 1\nunknown syntax\nlet second = 2\n}\n",
    ));
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM1202")
    );
    let children = &result.program.nodes[0].children;
    assert_eq!(children.len(), 2);
    assert_eq!(
        children[0].fields["name"].value,
        Value::Identifier("first".to_owned())
    );
    assert_eq!(
        children[1].fields["name"].value,
        Value::Identifier("second".to_owned())
    );
}

#[test]
fn missing_closer_keeps_partial_tree_and_reports_recovery_error() {
    let result = configured().parse(&SourceFile::new("open.lang", "program {\nlet item = 1\n"));
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM1204")
    );
    assert_eq!(result.program.nodes[0].children.len(), 1);
}

#[test]
fn nesting_and_node_limits_report_resource_errors_and_keep_the_root() {
    let source = SourceFile::new("limits.lang", "program {\nlet item = 1\n}\n");
    let depth = configured().parse_with_limits(
        &source,
        ParseLimits {
            max_nesting_depth: 0,
            ..ParseLimits::default()
        },
    );
    assert!(depth.resource_limited);
    assert!(
        depth
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM1205")
    );
    assert_eq!(depth.program.nodes.len(), 1);

    let nodes = configured().parse_with_limits(
        &source,
        ParseLimits {
            max_ast_nodes: 1,
            ..ParseLimits::default()
        },
    );
    assert!(nodes.resource_limited);
    assert!(
        nodes
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM1302")
    );
    assert_eq!(nodes.program.nodes.len(), 1);
    assert!(nodes.program.nodes[0].children.is_empty());
}

#[test]
fn recovery_balances_nested_blocks_with_different_openers_and_shared_closers() {
    let spec = LanguageBuilder::new("RecoveryBlocks")
        .version("0.1.0")
        .schema_version(2)
        .root("root")
        .construct(
            ConstructBuilder::new("root")
                .syntax("root")
                .block(["inner"])
                .block_delimiters("begin", "end"),
        )
        .construct(
            ConstructBuilder::new("inner")
                .syntax("inner")
                .block(["leaf"])
                .block_delimiters("[", "end"),
        )
        .construct(ConstructBuilder::new("leaf").syntax("leaf"))
        .build()
        .expect("recovery fixture validates");
    let parser = PreparedLanguage::new(spec)
        .expect("configured delimiters compile")
        .configure()
        .expect("fixture has no callbacks");
    let source = SourceFile::new("recovery.lang", "root begin\ninner [\nleaf\nend\nend\n");
    let parsed = parser.parse_with_limits(
        &source,
        ParseLimits {
            max_nesting_depth: 0,
            ..ParseLimits::default()
        },
    );
    assert!(parsed.resource_limited);
    assert_eq!(
        parsed.program.nodes[0].span.end,
        source.text.rfind("end").unwrap() + 3
    );
}

#[test]
fn caller_cannot_raise_recursive_parser_nesting_ceiling() {
    let spec = LanguageBuilder::new("DeepParserTests")
        .version("0.1.0")
        .root("node")
        .construct(
            ConstructBuilder::new("node")
                .syntax("node {")
                .block(["node"]),
        )
        .build()
        .expect("recursive test language is valid");
    let parser = PreparedLanguage::new(spec)
        .expect("syntax compiles")
        .configure()
        .expect("language has no callback registrations to validate");
    let mut source_text = "node {\n".repeat(langmeld_language::MAX_NESTING_DEPTH + 2);
    source_text.push_str(&"}\n".repeat(langmeld_language::MAX_NESTING_DEPTH + 2));

    let result = parser.parse_with_limits(
        &SourceFile::new("deep.lang", source_text),
        ParseLimits {
            max_nesting_depth: usize::MAX,
            ..ParseLimits::default()
        },
    );

    assert!(result.resource_limited);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM1205")
    );
    assert_eq!(result.program.nodes.len(), 1);
}

#[test]
fn parser_work_budget_is_shared_and_stops_with_a_stable_diagnostic() {
    let source = SourceFile::new("budget.lang", "program {\nlet item = 1\n}\n");
    let result = configured().parse_with_limits(
        &source,
        ParseLimits {
            max_pattern_steps: 6,
            ..ParseLimits::default()
        },
    );
    assert!(result.resource_limited);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM1304")
    );
    assert_eq!(
        result.program.nodes.len(),
        1,
        "the matched root remains in the partial result"
    );
}

#[test]
fn custom_parser_registry_is_checked_before_parsing() {
    let error = language()
        .with_custom_parsers([])
        .expect_err("declared callback is required");
    assert_eq!(error.issues.len(), 1);
    assert!(
        matches!(error.issues[0], langmeld_language::ParserRegistrationIssue::Missing(ref id) if id == "host-special")
    );

    let duplicate = CustomConstructParser::new("host-special", |_, _| Ok(None));
    let error = language()
        .with_custom_parsers([duplicate.clone(), duplicate])
        .expect_err("duplicate callback IDs are rejected");
    assert!(
        matches!(error.issues.as_slice(), [langmeld_language::ParserRegistrationIssue::Duplicate(id)] if id == "host-special")
    );

    let error = language()
        .with_custom_parsers([
            CustomConstructParser::new("host-special", |_, _| Ok(None)),
            CustomConstructParser::new("unused", |_, _| Ok(None)),
        ])
        .expect_err("undeclared callbacks are rejected");
    assert!(error.issues.iter().any(
        |issue| matches!(issue, langmeld_language::ParserRegistrationIssue::Unused(id) if id == "unused")
    ));
}

#[test]
fn custom_matches_keep_host_data_and_source_ranges() {
    let parser = language()
        .with_custom_parsers([CustomConstructParser::new(
            "host-special",
            |source, offset| {
                let Some(text) = source.text.get(offset..) else {
                    return Ok(None);
                };
                if !text.starts_with("special item") {
                    return Ok(None);
                }
                let name_start = offset + "special ".len();
                let end = name_start + "item".len();
                Ok(Some(
                    CustomConstructMatch::new(Span::new(offset, end), "native-special")
                        .with_field(
                            "name",
                            CapturedValue::new(
                                Value::Identifier("item".to_owned()),
                                Span::new(name_start, end),
                            ),
                        )
                        .with_host_data(73_u32),
                ))
            },
        )])
        .expect("callback registry is complete");
    let result = parser.parse(&SourceFile::new(
        "host.lang",
        "program {\nspecial item\n}\n",
    ));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let custom = &result.program.nodes[0].children[0];
    assert_eq!(custom.syntax_variant, "native-special");
    assert_eq!(custom.host_data::<u32>(), Some(&73));
    assert_eq!(custom.fields["name"].span, Span::new(18, 22));
    assert_eq!(result.custom_parser_spans, [custom.span]);
}

#[test]
fn invalid_custom_match_is_rejected_and_following_child_recovers() {
    let parser = language()
        .with_custom_parsers([CustomConstructParser::new("host-special", |_, offset| {
            Ok(Some(CustomConstructMatch::new(
                Span::new(offset, offset + 1),
                "custom",
            )))
        })])
        .expect("callback registry is complete");
    let result = parser.parse(&SourceFile::new(
        "invalid-host.lang",
        "program {\nspecial item\nlet safe = 1\n}\n",
    ));
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM1209")
    );
    assert_eq!(result.program.nodes[0].children.len(), 1);
    assert_eq!(result.program.nodes[0].children[0].construct, "binding");
}

#[test]
fn custom_parser_failures_retain_valid_diagnostics_and_custom_source_spans() {
    let parser = language()
        .with_custom_parsers([CustomConstructParser::new(
            "host-special",
            |source, offset| {
                if !source
                    .text
                    .get(offset..)
                    .is_some_and(|text| text.starts_with("special item"))
                {
                    return Ok(None);
                }
                Err(Diagnostic::error(
                    "HOST7001",
                    "host parser rejected this syntax",
                    Span::new(offset, offset + 12),
                ))
            },
        )])
        .expect("callback registry is complete");
    let result = parser.parse(&SourceFile::new(
        "host-error.lang",
        "program {\nspecial item\nlet safe = 1\n}\n",
    ));
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "HOST7001")
    );
    assert_eq!(result.custom_parser_spans, [Span::new(10, 22)]);
    assert_eq!(result.program.nodes[0].children.len(), 1);
    assert_eq!(result.program.nodes[0].children[0].construct, "binding");
}

#[test]
fn custom_lexer_can_supply_tokens_to_the_same_parser_engine() {
    let parser = configured().with_custom_lexer(langmeld_language::CustomLexer::new(|_| {
        langmeld_language::LexedSource::new(
            [
                LanguageToken::new(
                    LanguageTokenKind::Identifier("program".to_owned()),
                    Span::new(0, 7),
                ),
                LanguageToken::new(LanguageTokenKind::Punctuation('{'), Span::new(8, 9)),
                LanguageToken::new(LanguageTokenKind::Punctuation('}'), Span::new(9, 10)),
            ],
            [],
        )
    }));
    let result = parser.parse(&SourceFile::new("custom-lexer.lang", "program {}"));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.program.nodes[0].construct, "program");
    assert!(!result.resource_limited);
}

#[test]
fn longest_syntax_variant_wins_when_both_match_the_line() {
    let spec = LanguageBuilder::new("Longest")
        .version("0.1.0")
        .root("program")
        .construct(
            ConstructBuilder::new("program")
                .syntax("program {")
                .block(["binding"]),
        )
        .construct(
            ConstructBuilder::new("binding")
                .syntax_variant("plain", "let {name} = {value}")
                .syntax_variant("terminated", "let {name} = {value};")
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .field(FieldSpec::required("value", FieldKind::Number)),
        )
        .build()
        .expect("variants are valid");
    let parser = PreparedLanguage::new(spec)
        .expect("patterns compile")
        .configure()
        .expect("language has no custom parsers");
    let result = parser.parse(&SourceFile::new("long.lang", "program {\nlet x = 1;\n}\n"));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result.program.nodes[0].children[0].syntax_variant,
        "terminated"
    );
}

#[test]
fn equal_length_construct_matches_are_reported_as_ambiguous() {
    let spec = LanguageBuilder::new("Ambiguous")
        .version("0.1.0")
        .root("program")
        .construct(
            ConstructBuilder::new("program")
                .syntax("program {")
                .block(["special"]),
        )
        .construct(
            ConstructBuilder::new("special")
                .syntax("show {name}")
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .custom_parser("host-show"),
        )
        .build()
        .expect("custom parser and declarative pattern may share a start");
    let parser = PreparedLanguage::new(spec)
        .expect("patterns compile")
        .with_custom_parsers([CustomConstructParser::new("host-show", |source, offset| {
            if !source
                .text
                .get(offset..)
                .is_some_and(|text| text.starts_with("show item"))
            {
                return Ok(None);
            }
            Ok(Some(
                CustomConstructMatch::new(Span::new(offset, offset + 9), "host-show").with_field(
                    "name",
                    CapturedValue::new(
                        Value::Identifier("item".to_owned()),
                        Span::new(offset + 5, offset + 9),
                    ),
                ),
            ))
        })])
        .expect("custom parser is registered");
    let result = parser.parse(&SourceFile::new(
        "ambiguous.lang",
        "program {\nshow item\n}\n",
    ));
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM1203")
    );
    assert!(result.program.nodes[0].children.is_empty());
}

#[test]
fn opaque_tokens_support_host_parsing_without_generic_highlighting() {
    let spec = LanguageBuilder::new("OpaqueSource")
        .version("0.1.0")
        .root("source")
        .construct(
            ConstructBuilder::new("source")
                .custom_parser("opaque-source")
                .field(FieldSpec::required("name", FieldKind::Identifier)),
        )
        .build()
        .expect("opaque host parser model is valid");
    let parser = PreparedLanguage::new(spec)
        .expect("language prepares")
        .with_custom_parsers([CustomConstructParser::new(
            "opaque-source",
            |source, start| {
                Ok(Some(
                    CustomConstructMatch::new(Span::new(start, source.text.len()), "opaque")
                        .with_field(
                            "name",
                            CapturedValue::new(
                                Value::Identifier("source".to_owned()),
                                Span::new(start, start),
                            ),
                        ),
                ))
            },
        )])
        .expect("host parser registers")
        .with_custom_lexer(CustomLexer::new(|source| {
            LexedSource::new(
                [LanguageToken::new(
                    LanguageTokenKind::Opaque,
                    Span::new(0, source.text.len()),
                )],
                [],
            )
        }));
    let source = SourceFile::new("source", "host language syntax");
    let parsed = parser.parse(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.syntax_tokens[0].kind, SyntaxTokenKind::Opaque);
    assert!(classify_parse_result_prepared(parser.language(), &source, &parsed).is_empty());
}

#[test]
fn custom_parser_can_explicitly_own_an_empty_block() {
    let spec = LanguageBuilder::new("CustomBlock")
        .version("0.1.0")
        .root("program")
        .construct(
            ConstructBuilder::new("program")
                .syntax("program {")
                .block(["custom_block"]),
        )
        .construct(
            ConstructBuilder::new("custom_block")
                .syntax("CUSTOM {")
                .block(["binding"])
                .custom_parser("host-block"),
        )
        .construct(
            ConstructBuilder::new("binding")
                .syntax("let {name} = {value}")
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .field(FieldSpec::required("value", FieldKind::Number)),
        )
        .build()
        .expect("custom block definition is valid");
    let parser = PreparedLanguage::new(spec)
        .expect("patterns compile")
        .with_custom_parsers([CustomConstructParser::new(
            "host-block",
            |source, offset| {
                let Some(relative_close) =
                    source.text.get(offset..).and_then(|text| text.find('}'))
                else {
                    return Ok(None);
                };
                let end = offset + relative_close + 1;
                Ok(Some(
                    CustomConstructMatch::new(Span::new(offset, end), "host-block")
                        .with_children([]),
                ))
            },
        )])
        .expect("custom parser is registered");
    let result = parser.parse(&SourceFile::new(
        "custom-block.lang",
        "program {\nspecial {\nlet hidden = 1\n}\n}\n",
    ));
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let custom = &result.program.nodes[0].children[0];
    assert_eq!(custom.construct, "custom_block");
    assert!(custom.children.is_empty());
}

#[test]
fn custom_parser_ast_children_are_checked_and_retained() {
    let spec = LanguageBuilder::new("CustomChildren")
        .version("0.1.0")
        .root("program")
        .construct(
            ConstructBuilder::new("program")
                .syntax("program {")
                .block(["custom_block"]),
        )
        .construct(
            ConstructBuilder::new("custom_block")
                .syntax("CUSTOM {")
                .block(["binding"])
                .custom_parser("host-block"),
        )
        .construct(
            ConstructBuilder::new("binding")
                .syntax("let {name} = {value}")
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .field(FieldSpec::required("value", FieldKind::Number)),
        )
        .build()
        .expect("custom block definition is valid");
    let parser = PreparedLanguage::new(spec)
        .expect("patterns compile")
        .with_custom_parsers([CustomConstructParser::new(
            "host-block",
            |source, offset| {
                let Some(relative_close) =
                    source.text.get(offset..).and_then(|text| text.find('}'))
                else {
                    return Ok(None);
                };
                let end = offset + relative_close + 1;
                let Some(relative_name) = source
                    .text
                    .get(offset..end)
                    .and_then(|text| text.find("hidden"))
                else {
                    return Ok(None);
                };
                let name_start = offset + relative_name;
                let Some(relative_value) = source
                    .text
                    .get(name_start..end)
                    .and_then(|text| text.find('1'))
                else {
                    return Ok(None);
                };
                let value_start = name_start + relative_value;
                let child =
                    AstNode::new("binding", "host-binding", Span::new(offset + 10, end - 2))
                        .with_field(
                            "name",
                            CapturedValue::new(
                                Value::Identifier("hidden".to_owned()),
                                Span::new(name_start, name_start + 6),
                            ),
                        )
                        .with_field(
                            "value",
                            CapturedValue::new(
                                Value::Number(1.0),
                                Span::new(value_start, value_start + 1),
                            ),
                        );
                Ok(Some(
                    CustomConstructMatch::new(Span::new(offset, end), "host-block")
                        .with_children([child]),
                ))
            },
        )])
        .expect("custom parser is registered");
    let source = SourceFile::new(
        "custom-children.lang",
        "program {\nspecial {\nlet hidden = 1\n}\n}\n",
    );
    let result = parser.parse(&source);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let custom = &result.program.nodes[0].children[0];
    assert_eq!(custom.children.len(), 1);
    assert_eq!(custom.children[0].construct, "binding");
    assert_eq!(
        custom.children[0].fields["name"].value,
        Value::Identifier("hidden".to_owned())
    );

    let limited = parser.parse_with_limits(
        &source,
        ParseLimits {
            max_ast_nodes: 2,
            ..ParseLimits::default()
        },
    );
    assert!(limited.resource_limited);
    assert!(
        limited
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "LM1302")
    );
    assert_eq!(limited.program.nodes.len(), 1);
}
