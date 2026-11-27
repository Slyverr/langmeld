//! The lexer on syntax that real languages use.

mod support;

use langmeld_syntax::{
    Diagnostic, IdentStyle, IntPreset, LexCursor, LexLimits, Lexer, Preset, StringPreset,
    SyntaxKind, TokenSpec,
};
use support::*;

#[test]
fn unary_minus_right_after_an_operator() {
    let lexer = c_like();
    assert_eq!(
        significant(&lexer, "x=-1"),
        ["IDENT x", "EQ =", "MINUS -", "INT 1"]
    );
    assert_eq!(
        significant(&lexer, "a!=-b"),
        ["IDENT a", "NOT_EQ !=", "MINUS -", "IDENT b"]
    );
    assert_eq!(
        significant(&lexer, "x==-.5"),
        ["IDENT x", "EQ_EQ ==", "MINUS -", "DOT .", "INT 5"]
    );
}

#[test]
fn only_declared_operators_are_joined() {
    let lexer = c_like();
    // `>>` is not declared, so a parser can close two generic lists.
    assert_eq!(
        significant(&lexer, "Vec<Vec<T>>"),
        [
            "IDENT Vec",
            "LT <",
            "IDENT Vec",
            "LT <",
            "IDENT T",
            "GT >",
            "GT >"
        ]
    );
    let with_shift = Lexer::builder()
        .tokens(puncts())
        .token(TokenSpec::punct(SyntaxKind(500), ">>"))
        .token(TokenSpec::regex(WS, "whitespace", " +").skip())
        .build()
        .expect("valid");
    assert_eq!(
        significant(&with_shift, "> >> >>>"),
        ["GT >", "? >>", "? >>", "GT >"]
    );
}

#[test]
fn joint_tokens_have_nothing_between_them() {
    let lexer = c_like();
    let joint: Vec<bool> = lex(&lexer, ">> > >/**/>")
        .tokens
        .iter()
        .filter(|token| !token.trivia)
        .map(|token| token.joint)
        .collect();
    assert_eq!(joint, [true, false, false, false, false]);
}

#[test]
fn floats_integers_and_ranges() {
    let lexer = c_like();
    assert_eq!(significant(&lexer, "1.5e-3"), ["FLOAT 1.5e-3"]);
    assert_eq!(
        significant(&lexer, "2E+10 3e7"),
        ["FLOAT 2E+10", "FLOAT 3e7"]
    );
    assert_eq!(
        significant(&lexer, "1..=4"),
        ["INT 1", "DOT_DOT_EQ ..=", "INT 4"]
    );
    assert_eq!(
        significant(&lexer, "1..4"),
        ["INT 1", "DOT_DOT ..", "INT 4"]
    );
    // A float needs a digit after the point.
    assert_eq!(
        significant(&lexer, "1.foo"),
        ["INT 1", "DOT .", "IDENT foo"]
    );
    assert_eq!(significant(&lexer, "1_000.000_1"), ["FLOAT 1_000.000_1"]);
}

#[test]
fn integers_in_every_base() {
    let lexer = c_like();
    assert_eq!(
        significant(&lexer, "0xff_ff 0XA 0b10_1 0o17 007 1__0"),
        [
            "INT 0xff_ff",
            "INT 0XA",
            "INT 0b10_1",
            "INT 0o17",
            "INT 007",
            "INT 1__0"
        ]
    );
    assert_eq!(significant(&lexer, "0b12"), ["INT 0b1", "INT 2"]);
    assert_eq!(significant(&lexer, "0x"), ["INT 0x"]);
    assert_eq!(
        lex_errors(&lexer, "0x + 0b_"),
        [
            "LM-L006 0..2 `0x` needs at least one digit after it",
            "LM-L006 5..7 `0b` needs at least one digit after it",
        ]
    );
}

#[test]
fn integer_options_are_off_by_default() {
    let lexer = Lexer::builder()
        .token(TokenSpec::preset(
            INT,
            "integer",
            Preset::Int(IntPreset::default()),
        ))
        .token(TokenSpec::preset(
            IDENT,
            "identifier",
            Preset::Ident(IdentStyle::Ascii),
        ))
        .build()
        .expect("valid");
    assert_eq!(significant(&lexer, "1_0"), ["INT 1", "IDENT _0"]);
    assert_eq!(significant(&lexer, "0x1"), ["INT 0", "IDENT x1"]);
}

#[test]
fn the_longest_match_wins() {
    let lexer = c_like();
    assert_eq!(
        significant(&lexer, "700ms 7s 700"),
        ["DURATION 700ms", "DURATION 7s", "INT 700"]
    );
    assert_eq!(
        significant(&lexer, "left-to-right left - right"),
        ["BARE left-to-right", "IDENT left", "MINUS -", "IDENT right"]
    );
    assert_eq!(
        significant(&lexer, "a->b"),
        ["IDENT a", "ARROW ->", "IDENT b"]
    );
}

#[test]
fn keywords_are_reserved_and_contextual_words_are_identifiers() {
    let lexer = c_like();
    assert_eq!(
        significant(&lexer, "let letter fn fnord from let-x"),
        [
            "LET_KW let",
            "IDENT letter",
            "FN_KW fn",
            "IDENT fnord",
            "IDENT from",
            "BARE let-x"
        ]
    );
}

#[test]
fn character_literals() {
    let lexer = c_like();
    assert_eq!(
        significant(&lexer, r"'c' '\n' '\'' '\u{1F600}' '😀'"),
        [
            "CHAR 'c'",
            r"CHAR '\n'",
            r"CHAR '\''",
            r"CHAR '\u{1F600}'",
            "CHAR '😀'"
        ]
    );
    assert_eq!(
        lex_errors(&lexer, "''"),
        ["LM-L005 0..2 a character literal holds exactly one character"]
    );
    assert_eq!(
        lex_errors(&lexer, "'ab' x"),
        ["LM-L005 0..4 a character literal holds exactly one character"]
    );
    assert_eq!(
        lex_errors(&lexer, "'a\nx"),
        ["LM-L004 0..2 unterminated character literal; add a closing `'`"]
    );
    assert_eq!(significant(&lexer, "'a\nx"), ["CHAR 'a", "IDENT x"]);
}

#[test]
fn strings_keep_escapes_and_end_at_the_line_end() {
    let lexer = c_like();
    assert_eq!(
        significant(&lexer, r#""\u{1F600}" "a\"b" "" "😀//not a comment""#),
        [
            r#"STRING "\u{1F600}""#,
            r#"STRING "a\"b""#,
            r#"STRING """#,
            r#"STRING "😀//not a comment""#
        ]
    );
    for text in ["\"abc\nx", "\"abc\r\nx", "\"abc\rx"] {
        assert_eq!(
            significant(&lexer, text),
            ["STRING \"abc", "IDENT x"],
            "{text:?}"
        );
        assert_eq!(
            lex_errors(&lexer, text),
            ["LM-L002 0..4 unterminated string; add a closing `\"` before the end of the line"]
        );
    }
    // A backslash cannot carry a string over a line end.
    assert_eq!(significant(&lexer, "\"a\\\nb"), ["STRING \"a\\", "IDENT b"]);
}

#[test]
fn single_quoted_and_raw_strings() {
    let preset = StringPreset::default().double(true).single(true).raw(true);
    let lexer = Lexer::builder()
        .token(TokenSpec::preset(STRING, "string", Preset::String(preset)))
        .token(TokenSpec::preset(
            IDENT,
            "identifier",
            Preset::Ident(IdentStyle::Ascii),
        ))
        .token(TokenSpec::regex(WS, "whitespace", " +").skip())
        .build()
        .expect("valid");
    assert_eq!(
        significant(&lexer, r#"'a' "b" r"c\" r'\d' r x"#),
        [
            "STRING 'a'",
            "STRING \"b\"",
            r#"STRING r"c\""#,
            r"STRING r'\d'",
            "IDENT r",
            "IDENT x"
        ]
    );
    assert_eq!(
        lex_errors(&lexer, "'a"),
        ["LM-L002 0..2 unterminated string; add a closing `'` before the end of the line"]
    );
}

#[test]
fn line_and_nested_block_comments() {
    let lexer = c_like();
    assert_eq!(
        significant(&lexer, "a /* x /* nested */ still */ b // c\r\nd"),
        ["IDENT a", "IDENT b", "IDENT d"]
    );
    let kinds: Vec<_> = lex(&lexer, "///doc\n//line\n/**/")
        .tokens
        .iter()
        .map(|token| (kind_name(token.kind), token.trivia))
        .collect();
    assert_eq!(
        kinds,
        [
            ("DOC_COMMENT", true),
            ("WS", true),
            ("LINE_COMMENT", true),
            ("WS", true),
            ("BLOCK_COMMENT", true)
        ]
    );
    // The comment wins over the shorter `/` and `*`.
    assert_eq!(significant(&lexer, "a//b"), ["IDENT a"]);
    assert_eq!(
        significant(&lexer, "a/ /b"),
        ["IDENT a", "SLASH /", "SLASH /", "IDENT b"]
    );
}

#[test]
fn unterminated_comments_point_at_their_start() {
    let lexer = c_like();
    for text in ["x /* abc", "x /* /* abc */"] {
        assert_eq!(
            lex_errors(&lexer, text),
            ["LM-L003 2..4 unterminated block comment; add `*/`"],
            "{text:?}"
        );
        assert_eq!(significant(&lexer, text), ["IDENT x"]);
    }
}

#[test]
fn flat_comments_end_at_the_first_closer() {
    let lexer = Lexer::builder()
        .token(TokenSpec::block_comment(BLOCK_COMMENT, "/*", "*/"))
        .token(TokenSpec::preset(
            IDENT,
            "identifier",
            Preset::Ident(IdentStyle::Ascii),
        ))
        .token(TokenSpec::regex(WS, "whitespace", " +").skip())
        .build()
        .expect("valid");
    assert_eq!(significant(&lexer, "/* /* */ x"), ["IDENT x"]);
}

#[test]
fn an_operator_longer_than_a_comment_opener_is_an_operator() {
    let lexer = Lexer::builder()
        .token(TokenSpec::line_comment(LINE_COMMENT, "--"))
        .token(TokenSpec::punct(ARROW, "-->"))
        .token(TokenSpec::preset(
            IDENT,
            "identifier",
            Preset::Ident(IdentStyle::Ascii),
        ))
        .token(TokenSpec::regex(WS, "whitespace", " +").skip())
        .build()
        .expect("valid");
    assert_eq!(
        significant(&lexer, "a --> b -- c"),
        ["IDENT a", "ARROW -->", "IDENT b"]
    );
}

#[test]
fn a_byte_order_mark_is_trivia_at_the_start_only() {
    let lexer = c_like();
    let lexed = lex(&lexer, "\u{FEFF}x");
    assert_eq!(lexed.tokens[0].kind, SyntaxKind::BOM);
    assert!(lexed.tokens[0].trivia);
    assert!(lexed.diagnostics.is_empty());
    assert_eq!(
        lex_errors(&lexer, "x \u{FEFF}"),
        ["LM-L001 2..5 unexpected character `\\u{feff}`"]
    );
}

#[test]
fn every_kind_of_line_ending() {
    let lexer = c_like();
    assert_eq!(
        significant(&lexer, "a\r\nb\rc\nd"),
        ["IDENT a", "IDENT b", "IDENT c", "IDENT d"]
    );
}

#[test]
fn unicode_identifiers() {
    let lexer = c_like();
    assert_eq!(
        significant(&lexer, "héllo 名前 _x Ångström a1_"),
        [
            "IDENT héllo",
            "IDENT 名前",
            "IDENT _x",
            "IDENT Ångström",
            "IDENT a1_"
        ]
    );
    let ascii = Lexer::builder()
        .token(TokenSpec::preset(
            IDENT,
            "identifier",
            Preset::Ident(IdentStyle::Ascii),
        ))
        .build()
        .expect("valid");
    assert_eq!(significant(&ascii, "hé"), ["IDENT h", "ERROR é"]);
}

#[test]
fn a_run_of_unknown_characters_is_one_error() {
    let lexer = c_like();
    assert_eq!(
        lex_errors(&lexer, "a @ b"),
        ["LM-L001 2..3 unexpected character `@`"]
    );
    assert_eq!(
        lex_errors(&lexer, "a @#@ b 😀"),
        [
            "LM-L001 2..5 unexpected characters `@#@`",
            "LM-L001 8..12 unexpected character `😀`",
        ]
    );
    assert_eq!(
        lex_errors(&lexer, "\0"),
        ["LM-L001 0..1 unexpected character `\\0`"]
    );
    assert_eq!(
        significant(&lexer, "a @#@ b"),
        ["IDENT a", "ERROR @#@", "IDENT b"]
    );
}

fn kinds_of(lexer: &Lexer, text: &str) -> String {
    let words: Vec<_> = lex(lexer, text)
        .tokens
        .iter()
        .filter(|token| !token.trivia)
        .map(|token| match token.kind {
            NEWLINE => "NL",
            _ => &text[token.range],
        })
        .collect();
    words.join(" ")
}

#[test]
fn runs_of_blank_lines_and_comments_make_one_newline() {
    let lexer = terminated();
    assert_eq!(kinds_of(&lexer, "a\nb"), "a NL b");
    assert_eq!(
        kinds_of(&lexer, "\n\n// x\n\na  // y\n\n  // z\n\nb\n\n"),
        "a NL b NL"
    );
    assert_eq!(kinds_of(&lexer, "a /* \n */ b"), "a b");
    assert_eq!(kinds_of(&lexer, ""), "");
}

#[test]
fn a_newline_token_is_one_line_ending() {
    let lexer = terminated();
    for ending in ["\n", "\r\n", "\r"] {
        let text = format!("a  {ending}{ending}b");
        let newlines: Vec<_> = lex(&lexer, &text)
            .tokens
            .iter()
            .filter(|token| token.kind == NEWLINE)
            .map(|token| text[token.range].to_owned())
            .collect();
        assert_eq!(newlines, [ending]);
    }
}

#[test]
fn newlines_do_not_terminate_inside_brackets() {
    let lexer = terminated();
    assert_eq!(kinds_of(&lexer, "f(a\n, b\n)\nc"), "f ( a , b ) NL c");
    assert_eq!(kinds_of(&lexer, "x[\ny\n]\n"), "x [ y ] NL");
    // A block inside brackets terminates its own statements again.
    assert_eq!(kinds_of(&lexer, "f({a\nb\n}\n)"), "f ( { a NL b NL } )");
    // A stray closer changes nothing.
    assert_eq!(kinds_of(&lexer, ")\na\nb"), ") NL a NL b");
}

#[test]
fn a_line_ending_in_an_operator_or_comma_continues() {
    let lexer = terminated();
    assert_eq!(kinds_of(&lexer, "a +\nb,\n\nc\nd"), "a + b , c NL d");
    assert_eq!(kinds_of(&lexer, "a + // why\nb"), "a + b");
}

#[test]
fn interpolated_strings_become_parts() {
    let lexer = interpolating("{");
    assert_eq!(
        significant(&lexer, "\"a {x} b\""),
        [
            "STR_START \"",
            "STR_TEXT a ",
            "HOLE_OPEN {",
            "IDENT x",
            "HOLE_CLOSE }",
            "STR_TEXT  b",
            "STR_END \""
        ]
    );
    // Without holes, and without text between them.
    assert_eq!(
        significant(&lexer, "\"\" \"{a}{b}\""),
        [
            "STR_START \"",
            "STR_END \"",
            "STR_START \"",
            "HOLE_OPEN {",
            "IDENT a",
            "HOLE_CLOSE }",
            "HOLE_OPEN {",
            "IDENT b",
            "HOLE_CLOSE }",
            "STR_END \""
        ]
    );
}

#[test]
fn holes_track_nested_braces_and_strings() {
    let lexer = interpolating("{");
    assert_eq!(
        significant(&lexer, "\"{ {a} \"in{b}\" }!\""),
        [
            "STR_START \"",
            "HOLE_OPEN {",
            "L_BRACE {",
            "IDENT a",
            "R_BRACE }",
            "STR_START \"",
            "STR_TEXT in",
            "HOLE_OPEN {",
            "IDENT b",
            "HOLE_CLOSE }",
            "STR_END \"",
            "HOLE_CLOSE }",
            "STR_TEXT !",
            "STR_END \""
        ]
    );
}

#[test]
fn a_longer_hole_opener_leaves_single_braces_as_text() {
    let lexer = interpolating("${");
    assert_eq!(
        significant(&lexer, r#""{a} \${b} ${c}""#),
        [
            "STR_START \"",
            r"STR_TEXT {a} \${b} ",
            "HOLE_OPEN ${",
            "IDENT c",
            "HOLE_CLOSE }",
            "STR_END \""
        ]
    );
}

#[test]
fn an_unterminated_interpolated_string_is_one_error() {
    let lexer = interpolating("{");
    for text in ["\"ab\nc", "\"a{b\nc", "\"a{ \"{\nc"] {
        let errors = lex_errors(&lexer, text);
        assert_eq!(errors.len(), 1, "{text:?} {errors:?}");
        assert!(
            errors[0]
                .ends_with("unterminated string; add a closing `\"` before the end of the line"),
            "{errors:?}"
        );
        assert_eq!(
            significant(&lexer, text).last().map(String::as_str),
            Some("IDENT c")
        );
    }
    assert_eq!(
        lex_errors(&lexer, "x \"a{b"),
        ["LM-L002 2..6 unterminated string; add a closing `\"` before the end of the line"]
    );
}

/// `r#"..."#` needs counting, so it is lexed by hand.
fn raw_string(cursor: &mut LexCursor<'_>) -> Option<usize> {
    let after_r = cursor.rest().strip_prefix('r')?;
    let hashes = after_r.len() - after_r.trim_start_matches('#').len();
    let body = after_r.get(hashes..)?.strip_prefix('"')?;
    let closer = format!("\"{}", "#".repeat(hashes));
    let end = body.find(&closer)? + closer.len();
    Some(cursor.rest().len() - body.len() + end)
}

#[test]
fn external_tokens_are_lexed_by_rust() {
    let lexer = Lexer::builder()
        .token(TokenSpec::external(RAW_STRING, "raw string", raw_string))
        .token(TokenSpec::preset(
            IDENT,
            "identifier",
            Preset::Ident(IdentStyle::Ascii),
        ))
        .token(TokenSpec::regex(WS, "whitespace", " +").skip())
        .build()
        .expect("valid");
    assert_eq!(
        significant(&lexer, "r#\"a \"quoted\" word\"# r r\"x\""),
        [
            "RAW_STRING r#\"a \"quoted\" word\"#",
            "IDENT r",
            "RAW_STRING r\"x\""
        ]
    );
}

#[test]
fn a_wrong_answer_from_an_external_token_is_ignored() {
    // Zero bytes, too many bytes and half a character.
    for wrong in [0, 100, 1] {
        let lexer = Lexer::builder()
            .token(TokenSpec::external(
                RAW_STRING,
                "bad",
                move |_: &mut LexCursor<'_>| Some(wrong),
            ))
            .build()
            .expect("valid");
        assert_eq!(significant(&lexer, "é"), ["ERROR é"], "{wrong}");
    }
}

#[test]
fn an_external_token_sees_where_it_is() {
    let lexer = Lexer::builder()
        .token(TokenSpec::external(
            RAW_STRING,
            "second",
            |cursor: &mut LexCursor<'_>| {
                (u32::from(cursor.offset()) == 1 && cursor.source().starts_with('a')).then_some(1)
            },
        ))
        .token(TokenSpec::punct(DOT, "a"))
        .build()
        .expect("valid");
    assert_eq!(significant(&lexer, "ab"), ["DOT a", "RAW_STRING b"]);
}

/// Each definition error as `CODE message [notes]`.
fn definition_errors(specs: Vec<TokenSpec>) -> Vec<String> {
    match Lexer::builder().tokens(specs).build() {
        Ok(_) => Vec::new(),
        Err(errors) => errors.iter().map(describe).collect(),
    }
}

fn describe(error: &Diagnostic) -> String {
    format!(
        "{} {} [{}]",
        error.code,
        error.message,
        error.notes.join(" ")
    )
}

fn ident() -> TokenSpec {
    TokenSpec::preset(IDENT, "identifier", Preset::Ident(IdentStyle::Ascii))
}

#[test]
fn definitions_that_can_match_nothing_are_refused() {
    let fix = "To fix this, require at least one character, for example with `+` instead of `*`.";
    assert_eq!(
        definition_errors(vec![TokenSpec::regex(WS, "ws", " *")]),
        [format!("LM-G007 token `ws` can match empty text [{fix}]")]
    );
    assert_eq!(
        definition_errors(vec![TokenSpec::punct(WS, "")]),
        [format!("LM-G007 token `` can match empty text [{fix}]")]
    );
    assert_eq!(
        definition_errors(vec![TokenSpec::block_comment(WS, "/*", "")]),
        [format!("LM-G007 token `/*` can match empty text [{fix}]")]
    );
}

#[test]
fn definitions_that_can_never_match_are_refused() {
    assert_eq!(
        definition_errors(vec![
            TokenSpec::punct(EQ, "//"),
            TokenSpec::line_comment(LINE_COMMENT, "//"),
        ]),
        [
            "LM-G009 token `//` can never match, because another token is declared with the \
             same text [To fix this, remove one of the two.]"
        ]
    );
    assert_eq!(
        definition_errors(vec![
            ident(),
            TokenSpec::keyword(LET_KW, "let"),
            TokenSpec::keyword(FN_KW, "let"),
        ]),
        [
            "LM-G009 token `let` can never match, because another token is declared with the \
             same text [To fix this, remove one of the two.]"
        ]
    );
}

#[test]
fn two_strings_cannot_share_a_quote() {
    let string = |preset| TokenSpec::preset(STRING, "second", Preset::String(preset));
    assert_eq!(
        definition_errors(vec![
            TokenSpec::preset(CHAR, "first", Preset::String(double_quoted())),
            string(double_quoted()),
        ]),
        [
            "LM-G009 token `second` can never match, because `first` reads the same quotes \
             [To fix this, let one token enable that quote.]"
        ]
    );
    let single = StringPreset::default().single(true);
    assert!(
        definition_errors(vec![
            TokenSpec::preset(CHAR, "first", Preset::String(double_quoted())),
            string(single),
        ])
        .is_empty()
    );
}

#[test]
fn keywords_need_an_identifier_that_covers_them() {
    assert_eq!(
        definition_errors(vec![TokenSpec::keyword(LET_KW, "let")]),
        ["LM-L102 keyword `let` needs an identifier token \
             [To fix this, declare one, for example with the identifier preset.]"]
    );
    assert_eq!(
        definition_errors(vec![
            ident(),
            TokenSpec::regex(BARE, "bare word", "[a-z]+(-[a-z]+)+"),
            TokenSpec::keyword(LET_KW, "a-b"),
        ]),
        [
            "LM-G008 keyword `a-b` is not matched as an identifier [To fix this, declare it as \
             punctuation, or widen the identifier token so that it matches the whole word.]"
        ]
    );
}

#[test]
fn broken_patterns_are_refused() {
    let errors = definition_errors(vec![TokenSpec::regex(WS, "ws", "(")]);
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].starts_with("LM-L101 token `ws` has an invalid pattern: "),
        "{errors:?}"
    );
    assert!(
        errors[0].ends_with(" [To fix this, check the regular expression.]"),
        "{errors:?}"
    );

    assert_eq!(
        definition_errors(vec![TokenSpec::preset(
            STRING,
            "string",
            Preset::String(StringPreset::default())
        )]),
        ["LM-L104 string token `string` enables no kind of quote \
             [To fix this, enable `double` or `single`.]"]
    );
}

#[test]
fn patterns_that_are_too_large_are_refused() {
    let errors = definition_errors(vec![TokenSpec::regex(WS, "ws", r"\w{500}\w{500}")]);
    assert_eq!(
        errors,
        ["LM-L103 the token patterns are too complex to compile \
             [To fix this, simplify large repetitions such as `{1000}`.]"]
    );
}

#[test]
fn all_definition_errors_are_reported_together() {
    let errors = definition_errors(vec![
        TokenSpec::regex(WS, "a", ""),
        TokenSpec::regex(WS, "b", "("),
        TokenSpec::keyword(LET_KW, "let"),
    ]);
    assert_eq!(errors.len(), 3, "{errors:?}");
}

#[test]
fn files_over_the_size_limit_are_not_lexed() {
    let limits = LexLimits::default().max_len(4);
    let lexer = Lexer::builder()
        .tokens(puncts())
        .limits(limits)
        .build()
        .expect("valid");
    assert!(lex(&lexer, "....").diagnostics.is_empty());
    assert_eq!(
        lex_errors(&lexer, "....."),
        ["LM-L007 0..5 this file is 5 bytes long, and the limit is 4"]
    );
    assert_eq!(significant(&lexer, "....."), ["ERROR ....."]);
}

#[test]
fn lexing_stops_at_the_token_limit() {
    let limits = LexLimits::default().max_tokens(3);
    let lexer = Lexer::builder()
        .tokens(puncts())
        .limits(limits)
        .build()
        .expect("valid");
    assert!(lex(&lexer, ";;;").diagnostics.is_empty());
    assert_eq!(
        lex_errors(&lexer, ";;;;;"),
        ["LM-L008 3..5 this file has too many tokens; the limit is 3"]
    );
    assert_eq!(
        significant(&lexer, ";;;;;"),
        ["SEMI ;", "SEMI ;", "SEMI ;", "ERROR ;;"]
    );
}

#[test]
fn an_empty_lexer_still_covers_the_text() {
    let lexer = Lexer::builder().build().expect("valid");
    assert_eq!(significant(&lexer, "abc"), ["ERROR abc"]);
}

#[test]
fn any_short_text_can_be_lexed() {
    const PIECES: [&str; 18] = [
        "a", "1", "0x", ".", "\"", "'", "\\", "{", "}", "/*", "*/", "//", "\n", "\r", " ", "😀",
        "\u{FEFF}", "${",
    ];
    let lexers = [
        c_like(),
        terminated(),
        interpolating("{"),
        interpolating("${"),
    ];
    for len in 0..=3u32 {
        for mut number in 0..PIECES.len().pow(len) {
            let mut text = String::new();
            for _ in 0..len {
                text.push_str(PIECES[number % PIECES.len()]);
                number /= PIECES.len();
            }
            for lexer in &lexers {
                lex(lexer, &text);
            }
        }
    }
}
