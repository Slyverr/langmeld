use langmeld_core::{ConstructBuilder, FieldKind, FieldSpec, LanguageBuilder, SymbolSpec};
use langmeld_language::{generate_rust_ast, generate_tooling};

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
    assert_eq!(capabilities["highlighting"]["lexical_only"], true);
    assert!(
        capabilities["available_language_apis"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "source_formatter")
    );
    assert!(
        capabilities["available_language_apis"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "embedded_language_server")
    );
    assert_eq!(
        capabilities["editor_configuration_assumptions"]["line_comment"],
        "#"
    );
    assert!(
        capabilities["available_artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "language_configuration")
    );
    assert!(
        capabilities["editor_support"]["lsp_ranged_document_changes"]
            .as_bool()
            .unwrap()
    );
    assert!(
        capabilities["editor_support"]["lsp_literal_keyword_completion"]
            .as_bool()
            .unwrap()
    );
    assert!(
        capabilities["editor_support"]["lsp_parser_continuation_completion"]
            .as_bool()
            .unwrap()
    );
    assert!(
        capabilities["editor_support"]["lsp_document_symbols"]
            .as_bool()
            .unwrap()
    );
    assert!(
        capabilities["editor_support"]["lsp_navigation"]
            .as_bool()
            .unwrap()
    );
    assert!(
        capabilities["editor_support"]["lsp_navigation_open_documents_only"]
            .as_bool()
            .unwrap()
    );
    assert!(
        capabilities["editor_support"]["lsp_rename"]
            .as_bool()
            .unwrap()
    );
    assert!(
        capabilities["not_yet_available"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "unopened_project_file_navigation")
    );
    assert!(
        capabilities["optional_artifacts"]["tree_sitter"]["available"]
            .as_bool()
            .unwrap()
    );
    assert!(
        capabilities["not_yet_available"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "wasm_adapter")
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
    assert!(
        !capabilities["optional_artifacts"]["tree_sitter"]["available"]
            .as_bool()
            .unwrap()
    );
    assert!(
        capabilities["optional_artifacts"]["tree_sitter"]["reason"]
            .as_str()
            .unwrap()
            .contains("custom parser `host-custom`")
    );
}

#[test]
fn keyword_completion_capability_is_false_without_literal_keywords() {
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
        capabilities["editor_support"]["lsp_literal_keyword_completion"],
        false
    );
}
