use langmeld_core::{ConstructBuilder, FieldKind, FieldSpec, LanguageBuilder, MapKeyKind};
use langmeld_language::generate_tree_sitter;
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
                    "let {mode} {name} -> {value} tags {tags} attrs {attrs} config {config}",
                )
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
                        fields: vec![FieldSpec::optional("note", FieldKind::Text)],
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
    assert!(grammar.contains("source_file: $ => seq($.lm_construct_program"));
    assert!(grammar.contains(
        r#"lm_string: $ => token(seq("\"", repeat(choice(/[^"\\\r\n]/, /\\[nrt"\\]/)), "\""))"#
    ));
    assert!(grammar.contains("field(\"name\", $.lm_identifier)"));
    assert!(grammar.contains("field(\"value\", $.lm_number)"));
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
    assert!(highlights.contains("\"fast\" @constant.builtin"));
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

    for (path, contents) in generate_tree_sitter(&language()).expect("fixture exports") {
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
    fs::write(
        output.join("sample.demo"),
        "program {\n  let fast demo -> 42 tags [\"one\", \"two\"] attrs {count: \"2\"} config {note: \"sample\"}\n}\n",
    )
    .expect("sample program is written");
}
