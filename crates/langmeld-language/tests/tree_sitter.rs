use langmeld_core::{
    ConstructBuilder, FieldKind, FieldSpec, LanguageBuilder, MapKeyKind, SourceFile, SymbolSpec,
};
use langmeld_language::{PreparedLanguage, generate_tree_sitter, langmeld_definition_language};
use std::{env, fs, path::Path};

fn language() -> langmeld_core::ValidatedLanguageSpec {
    LanguageBuilder::new("Demo / Language")
        .version("0.2.0")
        .root("program")
        .named_enum("Mode", ["fast", "safe"])
        .construct(
            ConstructBuilder::new("program")
                .syntax_variant("root", "program {")
                .block(["binding"]),
        )
        .construct(
            ConstructBuilder::new("binding")
                .syntax_variant(
                    "binding",
                    "let {mode} {name} -> {value} tags {tags} attrs {attrs} config {config} status {status} owner {owner} flags {flags} labels {labels} history {history} metadata {metadata}",
                )
                .symbol(SymbolSpec::new("binding", "name"))
                .field(FieldSpec::required(
                    "mode",
                    FieldKind::named_enumeration("Mode", ["fast", "safe"]),
                ))
                .field(FieldSpec::required("name", FieldKind::Identifier))
                .field(FieldSpec::required("value", FieldKind::Number))
                .field(FieldSpec::required(
                    "tags",
                    FieldKind::list(FieldKind::Text),
                ))
                .field(FieldSpec::required(
                    "attrs",
                    FieldKind::map(MapKeyKind::Identifier, FieldKind::Text),
                ))
                .field(FieldSpec::required(
                    "config",
                    FieldKind::Properties {
                        name: None,
                        fields: vec![
                            FieldSpec::required("title", FieldKind::Text),
                            FieldSpec::optional("note", FieldKind::Text),
                        ],
                    },
                ))
                .field(FieldSpec::required(
                    "status",
                    FieldKind::Enum {
                        values: vec!["draft".to_owned(), "published".to_owned()],
                    },
                ))
                .field(FieldSpec::required(
                    "owner",
                    FieldKind::Reference {
                        target: "binding".to_owned(),
                    },
                ))
                .field(FieldSpec::required(
                    "flags",
                    FieldKind::list(FieldKind::Enum {
                        values: vec!["on".to_owned(), "off".to_owned()],
                    }),
                ))
                .field(FieldSpec::required(
                    "labels",
                    FieldKind::map(MapKeyKind::Text, FieldKind::Number),
                ))
                .field(FieldSpec::required(
                    "history",
                    FieldKind::list(FieldKind::Properties {
                        name: None,
                        fields: vec![FieldSpec::required("note", FieldKind::Text)],
                    }),
                ))
                .field(FieldSpec::required(
                    "metadata",
                    FieldKind::Properties {
                        name: None,
                        fields: Vec::new(),
                    },
                )),
        )
        .build()
        .expect("tree-sitter fixture validates")
}

#[test]
fn tree_sitter_export_is_deterministic_and_returns_a_complete_relative_artifact_set() {
    let language = language();
    let artifacts = generate_tree_sitter(&language).expect("supported patterns export");
    assert_eq!(
        artifacts,
        generate_tree_sitter(&language).expect("generation repeats")
    );
    assert_eq!(
        artifacts.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "tree-sitter/README.md",
            "tree-sitter/grammar.js",
            "tree-sitter/package.json",
            "tree-sitter/queries/highlights.scm",
            "tree-sitter/tree-sitter.json",
        ]
    );
    assert!(artifacts.keys().all(|path| {
        !path.starts_with('/') && !path.split('/').any(|component| component == "..")
    }));

    let grammar = &artifacts["tree-sitter/grammar.js"];
    assert!(grammar.contains("name: \"demo_language\""));
    assert!(grammar.contains("source_file: $ => seq(repeat($.lm_newline), $.lm_construct_program"));
    assert!(grammar.contains(
        r#"lm_string: $ => token(seq("\"", repeat(choice(/[^"\\\r\n]/, /\\[nrt"\\]/)), "\""))"#
    ));
    assert!(grammar.contains("field(\"name\", $.lm_identifier)"));
    assert!(grammar.contains("field(\"value\", $.lm_number)"));
    assert!(grammar.contains("field(\"owner\", $.lm_identifier)"));
    assert!(grammar.contains("field(\"attrs\", $.lm_type_"));
    assert!(grammar.contains(
        "choice(repeat($.lm_separator), seq(repeat($.lm_separator), $.lm_construct_binding"
    ));
    assert!(grammar.contains("repeat(seq(repeat($.lm_separator), $.lm_construct_binding))"));
    assert!(grammar.contains("choice(repeat($.lm_newline), seq(repeat($.lm_newline), $.lm_string"));
    assert!(grammar.contains("field(\"note\", seq(\"note\", \":\", $.lm_string))"));
    assert!(
        grammar.contains("field(\"key\", $.lm_identifier), \":\", field(\"value\", $.lm_string)")
    );
    assert!(grammar.contains("field(\"key\", $.lm_string), \":\", field(\"value\", $.lm_number)"));
    assert!(grammar.contains("seq(\"{\", repeat(choice(\",\", \";\", $.lm_newline)), \"}\")"));
    assert!(!grammar.contains("choice(\",\", $.lm_newline)"));
    assert!(artifacts["tree-sitter/README.md"].contains("conventional `#` line comments"));
    assert!(artifacts["tree-sitter/README.md"].contains("Langmeld validation"));
    assert!(
        artifacts["tree-sitter/README.md"].contains("langmeld generate --project-root <project>")
    );
    assert!(
        artifacts["tree-sitter/README.md"].contains("langmeld::language::generate_tree_sitter")
    );
    assert!(
        artifacts["tree-sitter/README.md"]
            .contains("copy this directory to a standalone grammar location")
    );

    let metadata: serde_json::Value =
        serde_json::from_str(&artifacts["tree-sitter/tree-sitter.json"])
            .expect("Tree-sitter metadata is JSON");
    assert_eq!(metadata["grammars"][0]["name"], "demo_language");
    let package: serde_json::Value = serde_json::from_str(&artifacts["tree-sitter/package.json"])
        .expect("package metadata is JSON");
    assert_eq!(package["name"], "tree-sitter-demo-language");

    let highlights = &artifacts["tree-sitter/queries/highlights.scm"];
    assert!(highlights.contains("\"let\" @keyword"));
    assert!(highlights.contains("\"->\" @operator"));
    assert!(highlights.contains("(lm_type_0) @constant.builtin"));
    assert!(highlights.contains("(lm_type_4) @constant.builtin"));
    assert!(highlights.contains("(lm_type_6) @constant.builtin"));
    assert!(!highlights.contains("\"fast\" @constant.builtin"));
    assert!(!highlights.contains("\"draft\" @constant.builtin"));
}

#[test]
fn unsupported_custom_parsers_fail_without_returning_partial_artifacts() {
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

    let error = generate_tree_sitter(&language).expect_err("custom parser is not representable");
    assert!(error.contains("custom parser `host-custom`"));
    assert!(error.contains("construct `custom`"));
}

#[test]
#[ignore = "emits a complex grammar fixture for the pinned Tree-sitter CLI check"]
fn write_tree_sitter_cli_validation_fixture() {
    let output = env::var_os("LANGMELD_TREE_SITTER_FIXTURE_DIR")
        .map(std::path::PathBuf::from)
        .expect("LANGMELD_TREE_SITTER_FIXTURE_DIR must name a temporary output directory");
    fs::create_dir_all(&output).expect("fixture output directory is created");

    let language = language();
    for (path, contents) in generate_tree_sitter(&language).expect("fixture exports") {
        let relative = path
            .strip_prefix("tree-sitter/")
            .expect("generated artifact is under tree-sitter/");
        let relative = Path::new(relative);
        assert!(
            relative
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_))),
            "generated path must remain relative: {}",
            relative.display()
        );
        let destination = output.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).expect("artifact parent directory is created");
        }
        fs::write(destination, contents).expect("generated artifact is written");
    }
    let samples = [
        (
            "complete.demo",
            "program {\n  let fast demo -> 42 tags [\"one\", \"two\"] attrs {count: \"2\"} config {title: \"sample\"; note: \"optional\"} status draft owner root flags [on, off] labels {\"build target\": 3} history [{note: \"created\"}] metadata {}\n}\n",
        ),
        (
            "empty-collections.demo",
            "program {\n  let safe empty -> 0 tags [] attrs {} config {title: \"empty\"} status published owner root flags [] labels {} history [] metadata {}\n}\n",
        ),
        (
            "multiline-properties.demo",
            "program {\n  let fast multiline -> 1 tags [\"one\",\n    \"two\",] attrs {\n    first: \"a\",\n    second: \"b\";\n  } config {\n    title: \"record\"\n    note: \"line separated\"\n  } status draft owner root flags [on] labels {\n    \"one\": 1,\n    \"two\": 2\n  } history [\n    {note: \"created\"},\n    {note: \"edited\"}\n  ] metadata {}\n}\n",
        ),
    ];
    let escaped_crlf_source = "# unicode comment λ\nprogram {\n  let fast escaped -> 1 tags [] attrs {} config {title: \"quoted \\\"word\\\" and \\\\ path\"; note: \"line\\nnext\"} status draft owner root flags [] labels {} history [] metadata {}\n}\n"
        .replace('\n', "\r\n");
    let parser = PreparedLanguage::new(language)
        .expect("fixture patterns compile")
        .configure()
        .expect("fixture parser configures");
    let samples_dir = output.join("samples");
    fs::create_dir_all(&samples_dir).expect("sample directory is created");
    for (name, source) in samples.into_iter().chain(std::iter::once((
        "crlf-comments-escapes.demo",
        escaped_crlf_source.as_str(),
    ))) {
        let parsed = parser.parse(&SourceFile::new(name, source));
        assert!(
            !parsed.resource_limited,
            "sample `{name}` stays within parser limits"
        );
        assert!(
            parsed.diagnostics.is_empty(),
            "sample `{name}` parses: {:#?}",
            parsed.diagnostics
        );
        fs::write(samples_dir.join(name), source).expect("sample program is written");
    }
    let invalid_source = "program {\n  let fast invalid -> 1 tags [] attrs {first: \"a\"\n    second: \"b\"} config {title: \"invalid\"} status draft owner root flags [] labels {} history [] metadata {}\n}\n";
    let invalid = parser.parse(&SourceFile::new("invalid-map.demo", invalid_source));
    assert!(
        !invalid.diagnostics.is_empty(),
        "Langmeld rejects map entries separated by a newline without a comma or semicolon"
    );
    let invalid_string_source = "program {\n  let fast invalid -> 1 tags [] attrs {} config {title: \"unsupported \\q escape\"} status draft owner root flags [] labels {} history [] metadata {}\n}\n";
    let invalid_string = parser.parse(&SourceFile::new(
        "invalid-string-escape.demo",
        invalid_string_source,
    ));
    assert!(
        !invalid_string.diagnostics.is_empty(),
        "Langmeld rejects unsupported string escapes"
    );

    let invalid_dir = output.join("invalid");
    fs::create_dir_all(&invalid_dir).expect("invalid sample directory is created");
    fs::write(
        invalid_dir.join("map-missing-separator.demo"),
        invalid_source,
    )
    .expect("invalid sample program is written");
    fs::write(
        invalid_dir.join("unsupported-string-escape.demo"),
        invalid_string_source,
    )
    .expect("invalid string sample is written");
}

#[test]
#[ignore = "emits Tree-sitter validation fixtures from Langmeld's own definition grammar"]
fn write_definition_meta_tree_sitter_validation_fixture() {
    let output = env::var_os("LANGMELD_DEFINITION_TREE_SITTER_FIXTURE_DIR")
        .map(std::path::PathBuf::from)
        .expect("LANGMELD_DEFINITION_TREE_SITTER_FIXTURE_DIR must name a temporary directory");
    fs::create_dir_all(&output).expect("fixture output directory is created");

    let language = langmeld_definition_language().expect("definition metagrammar validates");
    for (path, contents) in generate_tree_sitter(&language).expect("definition grammar exports") {
        let relative = path
            .strip_prefix("tree-sitter/")
            .expect("generated artifact is under tree-sitter/");
        let destination = output.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).expect("artifact parent directory is created");
        }
        fs::write(destination, contents).expect("generated artifact is written");
    }

    let samples = [
        (
            "language.lm",
            include_str!("../src/definition_meta/language.lm"),
        ),
        (
            "constructs/00_language.lm",
            include_str!("../src/definition_meta/constructs/00_language.lm"),
        ),
        (
            "constructs/01_types.lm",
            include_str!("../src/definition_meta/constructs/01_types.lm"),
        ),
        (
            "constructs/02_constructs.lm",
            include_str!("../src/definition_meta/constructs/02_constructs.lm"),
        ),
        (
            "constructs/03_fields.lm",
            include_str!("../src/definition_meta/constructs/03_fields.lm"),
        ),
        (
            "constructs/04_syntax.lm",
            include_str!("../src/definition_meta/constructs/04_syntax.lm"),
        ),
        (
            "constructs/05_symbols.lm",
            include_str!("../src/definition_meta/constructs/05_symbols.lm"),
        ),
    ];
    for (relative, contents) in samples {
        let destination = output.join("samples").join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).expect("sample parent directory is created");
        }
        fs::write(destination, contents).expect("definition sample is written");
    }
    let invalid = output.join("invalid/unknown-root.lm");
    fs::create_dir_all(invalid.parent().expect("invalid fixture has a parent"))
        .expect("invalid fixture directory is created");
    fs::write(invalid, "this is not a language definition\n")
        .expect("invalid definition sample is written");
}
