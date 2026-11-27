//! Loading languages from `.langmeld` files: what is accepted, and the full text of
//! what is refused, one diagnostic for each mistake.

#![cfg(not(target_arch = "wasm32"))]

use langmeld_meta::{Loaded, Source, load_language};
use langmeld_syntax::{FileId, SourceFile, parse};

const HEAD: &str =
    "langmeld 1\nlanguage T { extensions \"t\" }\ntokens { ident = preset ident(xid) }\n";

fn grammar(name: &str) -> String {
    let path = format!("{}/tests/grammars/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

fn refused(source: Source) -> String {
    match load_language(source) {
        Ok(loaded) => panic!("accepted, with warnings {:?}", loaded.language.warnings()),
        Err(error) => error.rendered(),
    }
}

fn refused_text(text: &str) -> String {
    refused(Source::text("t.langmeld", text))
}

fn accepted(text: &str) -> Loaded {
    load_language(Source::text("t.langmeld", text)).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn the_calc_example_of_the_format_is_accepted_as_written() {
    let loaded = accepted(&grammar("calc-format.langmeld"));
    assert_eq!(loaded.extensions, ["calc"]);
    assert_eq!(loaded.language.extensions().collect::<Vec<_>>(), ["calc"]);
    assert!(loaded.language.warnings().is_empty());
    // Every choice is settled by the next token.
    assert_eq!(loaded.language.max_lookahead(), 1);
    assert_eq!(loaded.bindings.rules.len(), 5);
    assert_eq!(loaded.editor.rules.len(), 4);
    let file = SourceFile::new(
        FileId::new(0),
        "a.calc",
        "let x = 1 + 2 * 3;\nfn f(a) { a }",
    )
    .expect("small");
    let parsed = parse(&loaded.language, &file);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
}

#[test]
fn the_ambiguous_block_is_refused_with_a_fix() {
    insta::assert_snapshot!(refused_text(&grammar("calc-ambiguous.langmeld")));
}

#[test]
fn a_file_without_its_version_is_refused() {
    insta::assert_snapshot!(refused_text("language T { }\nrule File = \"a\"\n"));
}

#[test]
fn a_version_from_the_future_is_refused() {
    insta::assert_snapshot!(refused_text(
        "langmeld 2\nlanguage T { }\nrule File = \"a\"\n"
    ));
}

#[test]
fn a_file_without_a_language_is_refused() {
    insta::assert_snapshot!(refused_text("langmeld 1\nrule File = \"a\"\n"));
}

#[test]
fn a_second_language_block_is_refused() {
    insta::assert_snapshot!(refused_text(&format!(
        "{HEAD}language U {{ }}\nrule File = ident\n"
    )));
}

#[test]
fn names_against_the_convention_get_a_rename_that_reaches_every_use() {
    let source = format!("{HEAD}rule file_x = Then:ident other\nrule other = ident\n");
    let error = load_language(Source::text("t.langmeld", source)).expect_err("two names");
    insta::assert_snapshot!(error.rendered());
    let rename = &error.diagnostics[0].fixes[0];
    assert_eq!(rename.title, "rename to `FileX`");
    let renames_other = &error.diagnostics[2].fixes[0];
    // The rule `other` is named where it is declared and where it is used.
    assert_eq!(renames_other.edits.len(), 2);
}

#[test]
fn a_mistake_in_a_token_name_is_one_diagnostic() {
    insta::assert_snapshot!(refused_text(&format!(
        "{HEAD}tokens {{ Number = preset int }}\nrule File = Number\n"
    )));
}

#[test]
fn an_unknown_name_points_at_its_use_with_a_suggestion() {
    insta::assert_snapshot!(refused_text(&format!(
        "{HEAD}rule File = Itm*\nrule Item = ident\n"
    )));
}

#[test]
fn problems_in_the_grammar_point_at_the_rule_that_causes_them() {
    insta::assert_snapshot!(refused_text(&format!(
        "{HEAD}rule File = A\nrule A = A \"+\" ident | ident\nrule Empty = (ident?)*\n"
    )));
}

#[test]
fn a_warning_points_at_its_rule() {
    let loaded = accepted(&format!("{HEAD}rule File = ident\nrule Spare = ident\n"));
    insta::assert_snapshot!(langmeld_meta::render_warnings(&loaded));
}

#[test]
fn presets_and_their_options_are_checked() {
    insta::assert_snapshot!(refused_text(
        "langmeld 1\nlanguage T { }\ntokens {\n a = preset foo\n b = preset int(hexa)\n c = preset ident(xid, wide)\n d = preset int(interpolate \"{\" \"}\" with X)\n}\nrule File = a b c d\n"
    ));
}

#[test]
fn a_bad_escape_and_a_bad_precedence_are_reported_where_written() {
    insta::assert_snapshot!(refused_text(&format!(
        "{HEAD}rule File = \"a\\q\" E\nexpr E {{ atom A = ident\n infix M \"+\" prec 0 left }}\n"
    )));
}

#[test]
fn a_syntax_mistake_is_one_diagnostic_with_a_fix() {
    insta::assert_snapshot!(refused_text(&format!(
        "{HEAD}rule File = ident )\nrule B = ident\n"
    )));
    insta::assert_snapshot!(refused_text(&format!(
        "{HEAD}rule File = \"unclosed\nrule B = ident\n"
    )));
}

#[test]
fn a_rule_with_nothing_after_its_equals_sign_says_it_has_no_body() {
    let expected = "error[LM-P001]: rule `Item` has no body
 --> t.langmeld:4:11
  |
4 | rule Item =
  |           ^
  = note: To fix this, write what `Item` matches after the `=`, or remove the rule.

";
    // Whatever comes next, the rule is the one mistake.
    for next in ["rule Other = ident\n", "tokens { more = preset int }\n", ""] {
        let found = refused_text(&format!("{HEAD}rule Item =\n{next}"));
        assert_eq!(found, expected, "{next}");
    }
}

#[test]
fn imports_are_relative_and_each_file_is_read_once() {
    let source = Source::text(
        "lang/main.langmeld",
        format!(
            "{HEAD}import \"parts/a.langmeld\"\nimport \"parts/b.langmeld\"\nrule File = A B\n"
        ),
    )
    .file(
        "lang/parts/a.langmeld",
        "langmeld 1\nimport \"common.langmeld\"\nrule A = Z\n",
    )
    .file(
        "lang/parts/b.langmeld",
        "langmeld 1\nimport \"common.langmeld\"\nrule B = Z\n",
    )
    .file("lang/parts/common.langmeld", "langmeld 1\nrule Z = ident\n");
    let loaded = load_language(source).unwrap_or_else(|error| panic!("{error}"));
    let names: Vec<_> = loaded
        .files
        .iter()
        .map(|file| file.name().to_owned())
        .collect();
    assert_eq!(
        names,
        [
            "lang/main.langmeld",
            "lang/parts/a.langmeld",
            "lang/parts/common.langmeld",
            "lang/parts/b.langmeld"
        ]
    );
}

#[test]
fn a_missing_import_and_a_cycle_name_their_files() {
    let source = Source::text(
        "main.langmeld",
        format!("{HEAD}import \"a.langmeld\"\nimport \"gone.langmeld\"\nrule File = ident\n"),
    )
    .file(
        "a.langmeld",
        "langmeld 1\nimport \"main.langmeld\"\nrule A = \"a\"\n",
    );
    insta::assert_snapshot!(refused(source));
}

#[test]
fn a_problem_in_an_imported_file_names_that_file() {
    let source = Source::text(
        "main.langmeld",
        format!("{HEAD}import \"a.langmeld\"\nrule File = A\n"),
    )
    .file("a.langmeld", "langmeld 1\nrule A = Missing\n");
    insta::assert_snapshot!(refused(source));
}

#[test]
fn custom_code_is_required_unless_the_grammar_is_read_generically() {
    let text = format!(
        "{HEAD}extern rule Pair -> File first \"(\"\nextern token Raw\nrule File = Pair Raw\n"
    );
    insta::assert_snapshot!(refused_text(&text));

    let loaded = load_language(Source::text("t.langmeld", text.clone()).generic())
        .unwrap_or_else(|error| panic!("{error}"));
    let file = SourceFile::new(FileId::new(0), "a", "( x").expect("small");
    let parsed = parse(&loaded.language, &file);
    assert_eq!(parsed.syntax().text(), "( x");
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .filter(|d| d.code == "LM-F012")
            .count(),
        1
    );

    let hosted = load_language(
        Source::text("t.langmeld", text)
            .extern_rule("Pair", |p| {
                let node = p.start();
                p.bump();
                node.complete(p, langmeld_syntax::SyntaxKind::ERROR);
            })
            .extern_token("Raw", |_| None),
    );
    assert!(hosted.is_ok());
}

#[test]
fn the_other_settings_of_the_language_block_reach_the_grammar() {
    let loaded = accepted(
        "langmeld 1\nlanguage T {\n extensions \"a\" \"b\"\n comments { line \"#\" doc \"##\" block \"<<\" \">>\" }\n terminators { newline \";\" }\n continue_inside { \"(\" \")\" }\n}\ntokens { skip ws = /[ \\t]+/\n word = preset ident(ascii)\n str = preset string(double, interpolate \"${\" \"}\" with Name)\n}\nrule File = terminated(Line)*\nrule Line = word str?\nrule Name = word\n",
    );
    assert_eq!(loaded.extensions, ["a", "b"]);
    let file = SourceFile::new(FileId::new(0), "a", "x \"a ${b}\" # note\ny\n").expect("small");
    let parsed = parse(&loaded.language, &file);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
}

mod rendering {
    use langmeld_meta::render;
    use langmeld_syntax::{
        Diagnostic, FileId, Fix, Label, SourceFile, TextEdit, TextRange, TextSize,
    };

    fn range(start: u32, end: u32) -> TextRange {
        TextRange::new(TextSize::new(start), TextSize::new(end))
    }

    #[test]
    fn labels_notes_and_fixes_are_shown_with_the_places_they_name() {
        let files = [
            SourceFile::new(FileId::new(0), "main.calc", "let x = 1;\nlet x = 2;\nx;\n")
                .expect("small"),
            SourceFile::new(FileId::new(1), "other.calc", "let a = 1;\n").expect("small"),
        ];
        let diagnostic = Diagnostic::error(
            "CALC010",
            FileId::new(0),
            range(15, 16),
            "`x` is defined twice",
        )
        .label(Label::new(
            FileId::new(0),
            range(4, 5),
            "first defined here",
        ))
        .label(Label::new(FileId::new(1), range(4, 5), "and `a` here"))
        .note("a name can be defined once in a block")
        .fix(Fix::new(
            "rename it to `y`",
            vec![TextEdit::new(FileId::new(0), range(15, 16), "y")],
        ));
        insta::assert_snapshot!(render(&files, &[diagnostic]));
    }
}

#[test]
fn text_with_a_wide_character_where_a_rule_name_ends_does_not_panic() {
    // Found by fuzzing: a place that ended inside a two-byte character.
    let text = String::from_utf8_lossy(
        b"langmexpe4ted `\x01m\nrule\xd7\xb6tem = +\" \" \"a\" | \"a\"right\x02",
    )
    .into_owned();
    let found = refused_text(&text);
    insta::assert_snapshot!(found);
}
