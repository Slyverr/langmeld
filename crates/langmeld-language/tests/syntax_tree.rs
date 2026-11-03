use langmeld_core::{
    ConstructBuilder, FieldKind, FieldSpec, LanguageBuilder, MapKeyKind, SourceFile, Span,
    SymbolSpec,
};
use langmeld_language::{
    ConfiguredLanguageParser, CustomLexer, LanguageTokenKind, LexedSource, MAX_NESTING_DEPTH,
    ParseLimits, PreparedLanguage, SyntaxElement, SyntaxNode, SyntaxNodeRole, SyntaxTokenKind,
    classify_parse_result_prepared, lex_source,
};

fn parser() -> ConfiguredLanguageParser {
    let properties = FieldKind::Properties {
        name: None,
        fields: vec![
            FieldSpec::required("title", FieldKind::Text),
            FieldSpec::required(
                "mode",
                FieldKind::Enum {
                    values: vec!["fast".to_owned(), "safe".to_owned()],
                },
            ),
            FieldSpec::required(
                "tags",
                FieldKind::List {
                    item: Box::new(FieldKind::Text),
                },
            ),
            FieldSpec::required(
                "attrs",
                FieldKind::Map {
                    key: MapKeyKind::Identifier,
                    value: Box::new(FieldKind::Text),
                },
            ),
        ],
    };
    let spec = LanguageBuilder::new("SyntaxTreeTests")
        .version("0.1.0")
        .root("box")
        .construct(ConstructBuilder::new("box").syntax("box {").block(["item"]))
        .construct(
            ConstructBuilder::new("item")
                .syntax("item {name} -> {target} config {config}")
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .field(FieldSpec::required(
                    "target",
                    FieldKind::Reference {
                        target: "node".to_owned(),
                    },
                ))
                .field(FieldSpec::required("config", properties))
                .symbol(SymbolSpec {
                    kind: "node".to_owned(),
                    name_field: "name".to_owned(),
                    scope: None,
                }),
        )
        .build()
        .expect("test language is valid");
    PreparedLanguage::new(spec)
        .expect("patterns compile")
        .configure()
        .expect("language has no host callbacks")
}

fn source_with_node() -> SourceFile {
    SourceFile::new(
        "tree.lm",
        "box {\r\n  # heading\r\n  item café -> café config { title: \"Café\", mode: fast, tags: [\"a\", \"b\"], attrs: {owner: \"me\"} }\r\n}\r\n",
    )
}

fn collect_token_indices(node: &SyntaxNode, output: &mut Vec<usize>) {
    for element in &node.children {
        match element {
            SyntaxElement::Node(child) => collect_token_indices(child, output),
            SyntaxElement::Token(index) => output.push(*index),
        }
    }
}

fn has_node(
    node: &SyntaxNode,
    role: SyntaxNodeRole,
    field: Option<&str>,
    index: Option<usize>,
) -> bool {
    (node.role == role && node.field.as_deref() == field && node.index == index)
        || node.children.iter().any(|element| match element {
            SyntaxElement::Node(child) => has_node(child, role, field, index),
            SyntaxElement::Token(_) => false,
        })
}

#[test]
fn syntax_tree_preserves_utf8_crlf_comments_and_every_token_once() {
    let parser = parser();
    let source = source_with_node();
    let result = parser.parse(&source);

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let reconstructed = result
        .syntax_tokens
        .iter()
        .filter(|token| token.span.start < token.span.end)
        .map(|token| &source.text[token.span.start..token.span.end])
        .collect::<String>();
    assert_eq!(reconstructed, source.text);
    assert!(result.syntax_tokens.iter().any(|token| {
        token.kind == SyntaxTokenKind::Newline
            && &source.text[token.span.start..token.span.end] == "\r\n"
    }));
    assert!(result.syntax_tokens.iter().any(|token| {
        token.kind == SyntaxTokenKind::Comment
            && &source.text[token.span.start..token.span.end] == "# heading"
    }));
    assert!(result.syntax_tokens.iter().any(|token| {
        token.kind == SyntaxTokenKind::Whitespace
            && source.text[token.span.start..token.span.end].contains("  ")
    }));

    let mut indices = Vec::new();
    collect_token_indices(&result.syntax_tree, &mut indices);
    assert_eq!(indices, (0..result.syntax_tokens.len()).collect::<Vec<_>>());
    assert!(result.syntax_tree.children.iter().any(|element| {
        matches!(element, SyntaxElement::Node(node) if node.construct.as_deref() == Some("box"))
    }));
    assert!(has_node(
        &result.syntax_tree,
        SyntaxNodeRole::Field,
        Some("config"),
        None
    ));
    assert!(has_node(
        &result.syntax_tree,
        SyntaxNodeRole::ListItem,
        Some("tags"),
        Some(0)
    ));
    assert!(has_node(
        &result.syntax_tree,
        SyntaxNodeRole::Property,
        Some("title"),
        None
    ));
    assert!(has_node(
        &result.syntax_tree,
        SyntaxNodeRole::MapEntry,
        Some("attrs"),
        Some(0)
    ));
    assert!(has_node(
        &result.syntax_tree,
        SyntaxNodeRole::MapKey,
        Some("attrs"),
        Some(0)
    ));
    assert!(has_node(
        &result.syntax_tree,
        SyntaxNodeRole::MapValue,
        Some("attrs"),
        Some(0)
    ));
}

#[test]
fn parse_result_classification_uses_semantic_roles_and_tree_fields() {
    let parser = parser();
    let source = source_with_node();
    let parsed = parser.parse(&source);
    let classifications = classify_parse_result_prepared(parser.language(), &source, &parsed);
    let categories = |spelling: &str| {
        classifications
            .iter()
            .filter(|item| source.text.get(item.span.start..item.span.end) == Some(spelling))
            .map(|item| item.kind)
            .collect::<Vec<_>>()
    };

    assert_eq!(categories("box"), ["keyword"]);
    assert_eq!(categories("item"), ["keyword"]);
    assert_eq!(categories("café"), ["declaration", "reference"]);
    assert_eq!(categories("title"), ["property"]);
    assert_eq!(categories("attrs"), ["property"]);
    assert_eq!(categories("owner"), ["property"]);
    assert_eq!(categories("fast"), ["enumMember"]);
    assert_eq!(categories("# heading"), ["comment"]);
}

#[test]
fn custom_lexer_gaps_are_lossless_and_classification_uses_its_tokens() {
    let parser = parser().with_custom_lexer(CustomLexer::new(|source| {
        let tokens = lex_source(source)
            .tokens
            .into_iter()
            .filter(|token| !matches!(token.kind, LanguageTokenKind::Comment))
            .collect::<Vec<_>>();
        LexedSource::new(tokens, [])
    }));
    let source = source_with_node();
    let result = parser.parse(&source);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);

    let reconstructed = result
        .syntax_tokens
        .iter()
        .filter(|token| token.span.start < token.span.end)
        .map(|token| &source.text[token.span.start..token.span.end])
        .collect::<String>();
    assert_eq!(reconstructed, source.text);
    assert!(
        !result
            .syntax_tokens
            .iter()
            .any(|token| token.kind == SyntaxTokenKind::Comment)
    );
    assert!(result.syntax_tokens.iter().any(|token| {
        token.kind == SyntaxTokenKind::Unknown
            && source.text[token.span.start..token.span.end].starts_with('#')
    }));
    let classified = classify_parse_result_prepared(parser.language(), &source, &result);
    assert!(!classified.iter().any(|item| item.kind == "comment"));
}

#[test]
fn incomplete_constructs_have_source_covering_tree_and_synthetic_closers() {
    let parser = parser();
    let source = SourceFile::new(
        "incomplete.lm",
        "box {\n  item a -> b config { title: \"x\", mode: fast, tags: [], attrs: {} }",
    );
    let result = parser.parse(&source);
    assert!(result.diagnostics.iter().any(|item| item.code == "LM1204"));
    assert!(result.syntax_tokens.iter().any(|token| {
        token.kind == SyntaxTokenKind::MissingRightBrace
            && token.span == Span::new(source.text.len(), source.text.len())
    }));
    let mut indices = Vec::new();
    collect_token_indices(&result.syntax_tree, &mut indices);
    assert_eq!(indices, (0..result.syntax_tokens.len()).collect::<Vec<_>>());
    assert_eq!(result.syntax_tree.span, Span::new(0, source.text.len()));
    assert_eq!(result.program.nodes[0].span.end, source.text.len());
    assert_eq!(
        result
            .syntax_tokens
            .iter()
            .filter(|token| token.span.start < token.span.end)
            .map(|token| &source.text[token.span.start..token.span.end])
            .collect::<String>(),
        source.text
    );
}

#[test]
fn rejected_input_still_returns_a_tree_covering_the_original_source() {
    let parser = parser();
    let source = SourceFile::new("large.lm", "ééé");
    let result = parser.parse_with_limits(
        &source,
        ParseLimits {
            max_source_bytes: 1,
            ..ParseLimits::default()
        },
    );
    assert!(result.resource_limited);
    assert_eq!(result.syntax_tokens.len(), 1);
    assert_eq!(result.syntax_tokens[0].kind, SyntaxTokenKind::Unknown);
    assert_eq!(
        result.syntax_tokens[0].span,
        Span::new(0, source.text.len())
    );
    assert_eq!(result.syntax_tree.span, Span::new(0, source.text.len()));
}

#[test]
fn token_limited_tail_and_empty_input_keep_valid_tree_coverage() {
    let parser = parser();
    let source = SourceFile::new(
        "limited.lm",
        "box {\n item café -> café config { title: \"x\", mode: fast, tags: [], attrs: {} }\n}\n",
    );
    let limited = parser.parse_with_limits(
        &source,
        ParseLimits {
            max_tokens: 3,
            ..ParseLimits::default()
        },
    );
    assert!(limited.resource_limited);
    assert_eq!(
        limited
            .syntax_tokens
            .iter()
            .filter(|token| token.span.start < token.span.end)
            .map(|token| &source.text[token.span.start..token.span.end])
            .collect::<String>(),
        source.text
    );
    assert!(
        limited
            .syntax_tokens
            .iter()
            .any(|token| token.kind == SyntaxTokenKind::Unknown)
    );

    let empty = SourceFile::new("empty.lm", "");
    let empty_result = parser.parse(&empty);
    assert!(empty_result.syntax_tokens.is_empty());
    assert_eq!(empty_result.syntax_tree.span, Span::new(0, 0));
}

#[test]
fn missing_list_bracket_has_a_zero_width_recovery_leaf() {
    let spec = LanguageBuilder::new("MissingBracketTests")
        .version("0.1.0")
        .root("root")
        .construct(
            ConstructBuilder::new("root")
                .syntax("root {")
                .block(["items"]),
        )
        .construct(
            ConstructBuilder::new("items")
                .syntax("items {values}")
                .field(FieldSpec::required(
                    "values",
                    FieldKind::List {
                        item: Box::new(FieldKind::Text),
                    },
                )),
        )
        .build()
        .expect("test language is valid");
    let parser = PreparedLanguage::new(spec)
        .expect("patterns compile")
        .configure()
        .expect("language has no host callbacks");
    let source = SourceFile::new("incomplete.lm", "root {\n  items [\"one\"");
    let result = parser.parse(&source);
    assert!(result.diagnostics.iter().any(|item| item.code == "LM1219"));
    assert!(result.syntax_tokens.iter().any(|token| {
        token.kind == SyntaxTokenKind::MissingRightBracket
            && token.span == Span::new(source.text.len(), source.text.len())
    }));
    let mut indices = Vec::new();
    collect_token_indices(&result.syntax_tree, &mut indices);
    assert_eq!(indices, (0..result.syntax_tokens.len()).collect::<Vec<_>>());
}

#[test]
fn deeply_nested_values_keep_tokens_without_unbounded_editor_tree_recursion() {
    let mut nested_kind = FieldKind::Number;
    let mut nested_source = "7".to_owned();
    for _ in 0..MAX_NESTING_DEPTH + 8 {
        nested_kind = FieldKind::List {
            item: Box::new(nested_kind),
        };
        nested_source.insert(0, '[');
        nested_source.push(']');
    }
    let spec = LanguageBuilder::new("DeepValueTests")
        .version("0.1.0")
        .root("program")
        .construct(
            ConstructBuilder::new("program")
                .syntax("program {")
                .block(["nested"]),
        )
        .construct(
            ConstructBuilder::new("nested")
                .syntax("nested {value}")
                .field(FieldSpec::required("value", nested_kind)),
        )
        .build()
        .expect("deep test language is valid");
    let parser = PreparedLanguage::new(spec)
        .expect("patterns compile")
        .configure()
        .expect("language has no host callbacks");
    let source = SourceFile::new(
        "deep-values.lm",
        format!("program {{\n nested {nested_source}\n}}\n"),
    );
    let parsed = parser.parse(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut indices = Vec::new();
    collect_token_indices(&parsed.syntax_tree, &mut indices);
    assert_eq!(indices, (0..parsed.syntax_tokens.len()).collect::<Vec<_>>());
    let classifications = classify_parse_result_prepared(parser.language(), &source, &parsed);
    assert!(!classifications.is_empty());
}
