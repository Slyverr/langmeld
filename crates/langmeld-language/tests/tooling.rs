use langmeld_core::{
    BlockSeparator, BlockSeparatorPolicy, ConstructBuilder, ExpressionAtomKind, ExpressionSpec,
    FieldKind, FieldSpec, LanguageBuilder, LexicalSpec, MapKeyKind, SymbolSpec,
};
use langmeld_language::{generate_rust_ast, generate_tooling, generate_tooling_capabilities};

fn language() -> langmeld_core::ValidatedLanguageSpec {
    LanguageBuilder::new("Demo / Language")
        .version("0.2.0")
        .root("program")
        .source_extension(".dl")
        .named_enum("Mode", ["fast", "safe"])
        .construct(
            ConstructBuilder::new("program")
                .syntax_variant("root", "program {")
                .block(["binding"]),
        )
        .construct(
            ConstructBuilder::new("binding")
                .syntax_variant("binding", "let {mode} {name} => {value}")
                .field(FieldSpec::required(
                    "mode",
                    FieldKind::named_enumeration("Mode", ["fast", "safe"]),
                ))
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .symbol(SymbolSpec::new("binding", "name"))
                .field(FieldSpec::required("value", FieldKind::Number)),
        )
        .build()
        .expect("tooling fixture language validates")
}

#[test]
fn generated_tooling_is_deterministic_and_describes_current_capabilities() {
    let language = language();
    let artifacts = generate_tooling(&language).expect("tooling artifacts generate");
    assert_eq!(
        artifacts,
        generate_tooling(&language).expect("generation repeats")
    );
    assert_eq!(artifacts["rust/ast.rs"], generate_rust_ast(&language));

    let expected_paths = [
        "README.md",
        "agent-skill/SKILL.md",
        "agent-skill/references/capabilities.json",
        "agent-skill/references/grammar.md",
        "highlighting.json",
        "language-configuration.json",
        "metadata.json",
        "rust/ast.rs",
        "syntaxes/demo-language.tmLanguage.json",
        "tree-sitter/README.md",
        "tree-sitter/grammar.js",
        "tree-sitter/package.json",
        "tree-sitter/queries/highlights.scm",
        "tree-sitter/tree-sitter.json",
    ];
    assert_eq!(
        artifacts.keys().map(String::as_str).collect::<Vec<_>>(),
        expected_paths
    );
    assert!(artifacts.keys().all(|path| {
        !path.starts_with('/') && !path.split('/').any(|component| component == "..")
    }));

    let metadata: serde_json::Value =
        serde_json::from_str(&artifacts["metadata.json"]).expect("metadata is JSON");
    assert_eq!(metadata["name"], "Demo / Language");
    assert_eq!(metadata["root"], "program");
    assert_eq!(metadata["source_extensions"][0], ".dl");

    let highlighting: serde_json::Value =
        serde_json::from_str(&artifacts["highlighting.json"]).expect("highlighting is JSON");
    assert_eq!(highlighting["lexical_only"], true);
    assert!(
        highlighting["keywords"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "let")
    );
    assert!(
        highlighting["operators"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "=>")
    );
    assert!(
        highlighting["enum_values"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "fast")
    );

    let capabilities: serde_json::Value =
        serde_json::from_str(&artifacts["agent-skill/references/capabilities.json"])
            .expect("capability metadata is JSON");
    assert_eq!(
        capabilities,
        serde_json::to_value(
            generate_tooling_capabilities(&language).expect("capabilities generate")
        )
        .expect("typed capabilities serialize")
    );
    assert_eq!(capabilities["schema_version"], 2);
    assert_eq!(capabilities["language"], "Demo / Language");
    assert_eq!(
        capabilities["artifacts"]["textmate_grammar"]["status"],
        "partial"
    );
    assert!(
        capabilities["artifacts"]["textmate_grammar"]["reason"]
            .as_str()
            .unwrap()
            .contains("lexical")
    );
    assert_eq!(
        capabilities["editor_configuration_assumptions"]["line_comment"],
        "#"
    );
    assert!(capabilities["artifacts"]["tree_sitter_grammar"]["status"] == "supported");
    assert!(capabilities["features"]["lsp.ranged_document_changes"]["status"] == "supported");
    assert!(capabilities["features"]["lsp.literal_keyword_completion"]["status"] == "supported");
    assert!(capabilities["features"]["lsp.parser_continuation_completion"]["status"] == "partial");
    assert!(
        capabilities["features"]["lsp.symbol_navigation"]["reason"]
            .as_str()
            .unwrap()
            .contains("configured project source snapshots")
    );
    assert_eq!(
        capabilities["features"]["host.execution"]["status"],
        "customized"
    );
    assert_eq!(
        capabilities["features"]["language.source_formatter"]["status"],
        "supported"
    );
    assert_eq!(
        capabilities["features"]["language.embedded_lsp_server"]["status"],
        "supported"
    );
    assert_eq!(
        capabilities["features"]["lsp.project_definition_reload"]["status"],
        "partial"
    );
    assert_eq!(
        capabilities["features"]["parser.custom_construct_callbacks"]["status"],
        "customized"
    );
    let skill = &artifacts["agent-skill/SKILL.md"];
    assert!(skill.contains("literal-keyword suggestions"));
    assert!(skill.contains("document symbols"));
    assert!(skill.contains("generic definition/references"));
    assert!(artifacts["README.md"].contains("`let {mode} {name} => {value}`"));

    let config: serde_json::Value = serde_json::from_str(&artifacts["language-configuration.json"])
        .expect("editor config is JSON");
    assert_eq!(config["comments"]["lineComment"], "#");
}

#[test]
fn generated_editor_artifacts_follow_the_declared_comment_policy() {
    let mut spec = language().spec().clone();
    spec.schema_version = 4;
    spec.lexical = Some(
        LexicalSpec::new()
            .line_comment("//")
            .block_comment("/*", "*/"),
    );
    let language = spec.validate().expect("comment policy is valid");
    let artifacts = generate_tooling(&language).expect("tooling still generates");
    let capabilities: serde_json::Value =
        serde_json::from_str(&artifacts["agent-skill/references/capabilities.json"])
            .expect("capabilities are JSON");

    let config: serde_json::Value = serde_json::from_str(&artifacts["language-configuration.json"])
        .expect("editor configuration is JSON");
    assert_eq!(config["comments"]["lineComment"], "//");
    assert_eq!(
        config["comments"]["blockComment"],
        serde_json::json!(["/*", "*/"])
    );
    assert_eq!(
        config["comments"]["blockComment"],
        capabilities["editor_configuration_assumptions"]["block_comment"]
    );
    assert_eq!(
        config["comments"]["lineComment"],
        capabilities["editor_configuration_assumptions"]["line_comment"]
    );

    let textmate: serde_json::Value =
        serde_json::from_str(&artifacts["syntaxes/demo-language.tmLanguage.json"])
            .expect("TextMate grammar is JSON");
    assert!(
        textmate["patterns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|pattern| {
                pattern["name"] == "comment.line" && pattern["match"] == "//[^\\r\\n]*"
            })
    );
    assert!(
        textmate["patterns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|pattern| {
                pattern["name"] == "comment.block"
                    && pattern["begin"] == "/\\*"
                    && pattern["end"] == "\\*/"
            })
    );

    assert_eq!(
        capabilities["editor_configuration_assumptions"]["line_comment"],
        "//"
    );
    assert_eq!(
        capabilities["artifacts"]["tree_sitter_grammar"]["status"],
        "unavailable"
    );
    assert_eq!(
        capabilities["artifacts"]["tree_sitter_grammar"]["reason"],
        "Tree-sitter export does not yet represent configured block comments"
    );
}

#[test]
fn unsupported_tree_sitter_does_not_block_baseline_tooling() {
    let language = LanguageBuilder::new("CustomParserLanguage")
        .version("0.1.0")
        .root("program")
        .construct(
            ConstructBuilder::new("program")
                .syntax("program {")
                .block(["custom"]),
        )
        .construct(
            ConstructBuilder::new("custom")
                .syntax("custom {value}")
                .field(FieldSpec::required("value", FieldKind::Identifier))
                .custom_parser("host-custom"),
        )
        .build()
        .expect("custom parser fixture validates");

    let artifacts = generate_tooling(&language).expect("baseline tooling remains available");
    assert!(artifacts.contains_key("metadata.json"));
    assert!(
        !artifacts
            .keys()
            .any(|path| path.starts_with("tree-sitter/"))
    );
    let capabilities: serde_json::Value =
        serde_json::from_str(&artifacts["agent-skill/references/capabilities.json"])
            .expect("capabilities are JSON");
    assert_eq!(
        capabilities["artifacts"]["tree_sitter_grammar"]["status"],
        "unavailable"
    );
    assert!(
        capabilities["artifacts"]["tree_sitter_grammar"]["reason"]
            .as_str()
            .unwrap()
            .contains("custom parser `host-custom`")
    );
    assert_eq!(
        capabilities["features"]["language.source_formatter"]["status"],
        "customized"
    );
}

#[test]
fn generated_editor_configuration_and_reference_include_configured_blocks() {
    let language = LanguageBuilder::new("ConfiguredBlocks")
        .version("0.1.0")
        .schema_version(2)
        .root("program")
        .construct(
            ConstructBuilder::new("program")
                .syntax("program")
                .block(["statement"])
                .block_delimiters("begin", "end")
                .block_separators(
                    [BlockSeparator::literal(";")],
                    BlockSeparatorPolicy::Required,
                ),
        )
        .construct(ConstructBuilder::new("statement").syntax("step"))
        .build()
        .expect("configured block language validates");
    let artifacts = generate_tooling(&language).expect("tooling generation succeeds");
    let configuration: serde_json::Value =
        serde_json::from_str(&artifacts["language-configuration.json"])
            .expect("editor configuration is valid JSON");

    assert!(
        configuration["brackets"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(["begin", "end"]))
    );
    assert!(artifacts["README.md"].contains("Block delimiters: `begin` and `end`"));
    assert!(artifacts["README.md"].contains("Child separators: `;` (required)"));
}

#[test]
fn generated_bracket_pairs_follow_declared_structural_fields() {
    let language = LanguageBuilder::new("StructuralPairs")
        .version("0.1.0")
        .schema_version(3)
        .root("program")
        .expression(ExpressionSpec::new("value").atom(ExpressionAtomKind::Identifier))
        .construct(
            ConstructBuilder::new("program")
                .syntax("program")
                .block(["statement"])
                .block_delimiters("begin", "end"),
        )
        .construct(
            ConstructBuilder::new("statement")
                .syntax("statement {items} {attributes} {config} {value};")
                .field(FieldSpec::required(
                    "items",
                    FieldKind::list(FieldKind::Number),
                ))
                .field(FieldSpec::required(
                    "attributes",
                    FieldKind::map(MapKeyKind::Identifier, FieldKind::Number),
                ))
                .field(FieldSpec::required(
                    "config",
                    FieldKind::properties([FieldSpec::required("label", FieldKind::Text)]),
                ))
                .field(FieldSpec::required(
                    "value",
                    FieldKind::expression("value", ";"),
                )),
        )
        .build()
        .expect("structural field declarations validate");
    let artifacts = generate_tooling(&language).expect("tooling generation succeeds");
    let configuration: serde_json::Value =
        serde_json::from_str(&artifacts["language-configuration.json"])
            .expect("editor configuration is valid JSON");
    let expected_pairs = serde_json::json!([["(", ")"], ["[", "]"], ["begin", "end"], ["{", "}"]]);

    assert_eq!(configuration["brackets"], expected_pairs);
    let capabilities: serde_json::Value =
        serde_json::from_str(&artifacts["agent-skill/references/capabilities.json"])
            .expect("capability metadata is valid JSON");
    assert_eq!(
        capabilities["editor_configuration_assumptions"]["bracket_pairs"],
        expected_pairs
    );
}

#[test]
fn generated_bracket_pairs_omit_unused_conventional_defaults() {
    let language = LanguageBuilder::new("NoStructuralPairs")
        .version("0.1.0")
        .schema_version(2)
        .root("program")
        .construct(
            ConstructBuilder::new("program")
                .syntax("program")
                .block(["statement"])
                .block_delimiters("begin", "end"),
        )
        .construct(ConstructBuilder::new("statement").syntax("statement"))
        .build()
        .expect("minimal language validates");
    let artifacts = generate_tooling(&language).expect("tooling generation succeeds");
    let configuration: serde_json::Value =
        serde_json::from_str(&artifacts["language-configuration.json"])
            .expect("editor configuration is valid JSON");

    assert_eq!(
        configuration["brackets"],
        serde_json::json!([["begin", "end"]])
    );
}

#[test]
fn keyword_completion_is_unavailable_without_literal_keywords() {
    let language = LanguageBuilder::new("BareCapture")
        .version("0.1.0")
        .root("name")
        .construct(
            ConstructBuilder::new("name")
                .syntax("{value} {")
                .block(["child"])
                .field(FieldSpec::required("value", FieldKind::Identifier)),
        )
        .construct(
            ConstructBuilder::new("child")
                .syntax("{value}")
                .field(FieldSpec::required("value", FieldKind::Identifier)),
        )
        .build()
        .expect("capture-only language validates");
    let artifacts = generate_tooling(&language).expect("tooling artifacts generate");
    let capabilities: serde_json::Value =
        serde_json::from_str(&artifacts["agent-skill/references/capabilities.json"])
            .expect("capabilities are JSON");
    assert_eq!(
        capabilities["features"]["lsp.literal_keyword_completion"]["status"],
        "unavailable"
    );
}
