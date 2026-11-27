//! The Rust types written for a language: their text, and that it is formatted.

#![cfg(not(target_arch = "wasm32"))]

use std::io::Write;
use std::process::{Command, Stdio};

use langmeld_meta::codegen::generate;
use langmeld_meta::{Source, load_language};

fn calc() -> String {
    let path = format!(
        "{}/tests/grammars/calc-format.langmeld",
        env!("CARGO_MANIFEST_DIR")
    );
    let loaded = load_language(Source::path(path)).expect("the example is valid");
    generate(&loaded.language)
        .expect("types can be written")
        .source
}

#[test]
fn the_types_of_calc() {
    insta::assert_snapshot!(calc());
}

#[test]
fn the_types_are_formatted_the_way_rustfmt_does() {
    let source = calc();
    let mut rustfmt = Command::new("rustfmt")
        .args(["--edition", "2024", "--emit", "stdout"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("rustfmt runs");
    rustfmt
        .stdin
        .take()
        .expect("piped")
        .write_all(source.as_bytes())
        .expect("written");
    let formatted = rustfmt.wait_with_output().expect("finishes");
    assert!(formatted.status.success());
    assert_eq!(String::from_utf8_lossy(&formatted.stdout), source);
}

mod builder {
    use langmeld_meta::codegen::{CodegenError, generate};
    use langmeld_syntax::pattern::{field, lit, many, opt, r};
    use langmeld_syntax::{
        ExternRule, IdentStyle, IntPreset, Language, LanguageBuilder, Preset, Token, seq,
    };

    fn basics() -> LanguageBuilder {
        Language::builder("Things")
            .token(Token::skip("space", r"\s+"))
            .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
            .token(Token::preset("int", Preset::Int(IntPreset::default())))
    }

    fn source(builder: LanguageBuilder) -> String {
        let language = builder.build().expect("the grammar is valid");
        generate(&language).expect("types can be written").source
    }

    #[test]
    fn labels_that_are_words_of_rust_or_of_the_views_get_names_that_work() {
        let code = source(basics().rule(
            "File",
            seq![
                field("type", r("word")),
                field("syntax", r("word")),
                field("self", r("word")),
                lit(";")
            ],
        ));
        assert!(code.contains("pub fn r#type(&self)"), "{code}");
        assert!(code.contains("pub fn syntax_(&self)"), "{code}");
        assert!(code.contains("pub fn self_(&self)"), "{code}");
    }

    #[test]
    fn parts_that_a_tree_cannot_tell_apart_get_no_method_and_a_warning() {
        let language = basics()
            .rule(
                "File",
                seq![
                    lit("lost"),
                    opt(field("maybe", r("int"))),
                    field("always", r("int"))
                ],
            )
            .build()
            .expect("the grammar is valid");
        let code = generate(&language).expect("types can be written");
        assert!(!code.source.contains("pub fn maybe"), "{}", code.source);
        assert!(!code.source.contains("pub fn always"), "{}", code.source);
        assert_eq!(
            code.warnings,
            [
                "`File` gets no method for `maybe`: a tree cannot tell it apart from other \
                 parts; separate them with a literal or put one in a rule of its own",
                "`File` gets no method for `always`: a tree cannot tell it apart from other \
                 parts; separate them with a literal or put one in a rule of its own",
            ]
        );
    }

    #[test]
    fn a_node_called_visitor_would_clash_with_the_visitor_and_is_refused() {
        let language = basics()
            .rule("File", many(r("Visitor")))
            .rule("Visitor", field("name", r("word")))
            .build()
            .expect("the grammar is valid");
        let error = generate(&language).expect_err("refused");
        assert_eq!(error, CodegenError::Clash("Visitor".to_owned()));
        assert_eq!(
            error.to_string(),
            "two parts of the grammar would both be called `Visitor` in Rust; rename one"
        );
    }

    #[test]
    fn a_token_and_a_rule_with_the_same_name_in_rust_are_refused() {
        let language = basics()
            .rule("File", many(r("WordToken")))
            .rule("WordToken", field("name", r("word")))
            .build()
            .expect("the grammar is valid");
        assert_eq!(
            generate(&language).expect_err("refused"),
            CodegenError::Clash("WordToken".to_owned())
        );
    }

    #[test]
    fn a_language_with_custom_code_is_given_its_source_to_add_that_code_to() {
        let language = basics()
            .extern_rule(ExternRule::new("Raw", |_| {}).first(["raw"]))
            .rule("File", many(r("Raw")))
            .build()
            .expect("the grammar is valid");
        let code = generate(&language).expect("types can be written").source;
        assert!(code.contains("pub fn source()"), "{code}");
        assert!(!code.contains("pub fn language()"), "{code}");
        assert!(code.contains("`Raw`"), "{code}");
    }
}
