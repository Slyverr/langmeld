//! The parser primitives, used by a small hand-written parser.

mod support;

use langmeld_syntax::{
    Diagnostic, Marker, Parse, ParseLimits, Parser, SyntaxKind, SyntaxNode, TreeBuilder,
};
use rowan::NodeOrToken;
use support::*;

const SHR: SyntaxKind = SyntaxKind(900);

/// `file = (let | expr ";")*`
fn file(p: &mut Parser<'_>) {
    let m = p.start();
    while !p.at_end() {
        statement(p);
    }
    m.complete(p, FILE);
}

fn statement(p: &mut Parser<'_>) {
    match p.current() {
        LET_KW => let_statement(p),
        INT | IDENT | L_PAREN | MINUS => {
            expression(p);
            p.expect(SEMI);
        }
        L_BRACE => block(p),
        _ => p.recover("a statement", &[LET_KW, SEMI]),
    }
    p.eat(SEMI);
}

fn block(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    while !p.at(R_BRACE) && !p.at_end() {
        statement(p);
    }
    p.expect(R_BRACE);
    m.complete(p, BLOCK);
}

/// `let = "let" name (":" type)? "=" expr ";"`
fn let_statement(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    name(p, NAME);
    if p.eat(COLON) {
        ty(p);
    }
    p.expect(EQ);
    expression(p);
    p.expect(SEMI);
    m.complete(p, LET_STMT);
}

fn name(p: &mut Parser<'_>, node: SyntaxKind) {
    let m = p.start();
    p.expect(IDENT);
    m.complete(p, node);
}

/// `type = name ("<" type ("," type)* ">")?`
fn ty(p: &mut Parser<'_>) {
    let m = p.start();
    name(p, NAME_REF);
    if p.eat(LT) {
        ty(p);
        while p.eat(COMMA) {
            ty(p);
        }
        p.expect(GT);
    }
    m.complete(p, TYPE);
}

fn expression(p: &mut Parser<'_>) {
    binary(p, 0);
}

fn binary(p: &mut Parser<'_>, min: u8) {
    let Some(mut left) = operand(p) else { return };
    loop {
        // `current` and `nth` do not count as expected tokens.
        let (kind, power, tokens) = if p.at_glued(&[GT, GT]) {
            (Some(SHR), 4, 2)
        } else {
            match p.current() {
                EQ_EQ | LT | GT => (None, 1, 1),
                PLUS | MINUS => (None, 5, 1),
                STAR | SLASH => (None, 6, 1),
                _ => break,
            }
        };
        if power <= min {
            break;
        }
        let m = left.precede(p);
        match kind {
            Some(kind) => p.bump_glued(kind, tokens),
            None => p.bump(),
        }
        binary(p, power);
        left = m.complete(p, BINARY);
    }
}

fn operand(p: &mut Parser<'_>) -> Option<langmeld_syntax::CompletedMarker> {
    let m = p.start();
    let mut done = match p.current() {
        INT => {
            p.bump();
            m.complete(p, LITERAL)
        }
        IDENT => {
            p.bump();
            m.complete(p, NAME_REF)
        }
        MINUS => {
            p.bump();
            operand(p);
            m.complete(p, BINARY)
        }
        L_PAREN => {
            p.bump();
            expression(p);
            p.expect(R_PAREN);
            m.complete(p, PAREN)
        }
        _ => {
            m.abandon(p);
            p.expected("an expression");
            return None;
        }
    };
    while p.current() == L_PAREN {
        let m = done.precede(p);
        p.bump();
        while !p.at(R_PAREN) && !p.at_end() {
            expression(p);
            if p.eat(COMMA) {
                continue;
            }
            if p.at(R_PAREN) {
                break;
            }
            p.error_expected();
            // A missing comma: go on if another argument starts here.
            if !matches!(p.current(), INT | IDENT | L_PAREN | MINUS) {
                break;
            }
        }
        p.expect(R_PAREN);
        done = m.complete(p, CALL);
    }
    Some(done)
}

fn parse_with(text: &str, limits: Option<ParseLimits>, grammar: fn(&mut Parser<'_>)) -> Parse {
    let lexer = c_like();
    let file = source(text);
    let mut parser = Parser::new(&file, lex(&lexer, text), &lexer);
    if let Some(limits) = limits {
        parser = parser.limits(limits);
    }
    grammar(&mut parser);
    let parse = parser.finish();
    assert_eq!(
        parse.syntax().text().to_string(),
        text,
        "the tree is lossless"
    );
    for diagnostic in &parse.diagnostics {
        assert!(
            u32::from(diagnostic.range.end()) as usize <= text.len(),
            "{diagnostic:?}"
        );
    }
    parse
}

fn parse(text: &str) -> Parse {
    parse_with(text, None, file)
}

/// The tree on one line, without trivia.
fn shape(node: &SyntaxNode) -> String {
    let mut out = format!("({}", kind_name_of(node.kind()));
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(node) => {
                out.push(' ');
                out.push_str(&shape(&node));
            }
            NodeOrToken::Token(token) => {
                if !matches!(
                    token.kind(),
                    WS | LINE_COMMENT | BLOCK_COMMENT | DOC_COMMENT
                ) {
                    out.push(' ');
                    out.push_str(token.text());
                }
            }
        }
    }
    out.push(')');
    out
}

fn kind_name_of(kind: SyntaxKind) -> &'static str {
    if kind == SHR { "SHR" } else { kind_name(kind) }
}

fn shape_of(text: &str) -> String {
    shape(&parse(text).syntax())
}

fn problems(diagnostics: &[Diagnostic]) -> Vec<String> {
    errors(diagnostics)
}

fn errors_of(text: &str) -> Vec<String> {
    problems(&parse(text).diagnostics)
}

#[test]
fn builds_a_tree() {
    assert_eq!(
        shape_of("let x = 1;"),
        "(FILE (LET_STMT let (NAME x) = (LITERAL 1) ;))"
    );
    assert!(errors_of("let x = 1;").is_empty());
    assert_eq!(shape_of(""), "(FILE)");
}

#[test]
fn precede_builds_left_associative_trees() {
    assert_eq!(
        shape_of("1+2+3*4;"),
        "(FILE (BINARY (BINARY (LITERAL 1) + (LITERAL 2)) + (BINARY (LITERAL 3) * (LITERAL 4))) ;)"
    );
    assert_eq!(
        shape_of("f(a)(b);"),
        "(FILE (CALL (CALL (NAME_REF f) ( (NAME_REF a) )) ( (NAME_REF b) )) ;)"
    );
}

#[test]
fn unary_minus_after_an_operator() {
    assert_eq!(
        shape_of("1 - -2;"),
        "(FILE (BINARY (LITERAL 1) - (BINARY - (LITERAL 2))) ;)"
    );
}

#[test]
fn angle_brackets_close_generics_one_by_one_and_join_into_a_shift() {
    assert_eq!(
        shape_of("let a: Vec<Vec<T>> = b >> c;"),
        "(FILE (LET_STMT let (NAME a) : (TYPE (NAME_REF Vec) < (TYPE (NAME_REF Vec) < \
         (TYPE (NAME_REF T)) >) >) = (BINARY (NAME_REF b) >> (NAME_REF c)) ;))"
    );
    // Apart, they are two comparisons.
    assert_eq!(
        errors_of("b > > c;"),
        ["LM-P001 4..5 expected an expression, found `>`"]
    );
    let shift = parse("b >> c;").syntax();
    let joined = shift
        .descendants_with_tokens()
        .find(|element| element.kind() == SHR)
        .expect("the tokens were joined");
    assert_eq!(joined.to_string(), ">>");
}

#[test]
fn a_missing_semicolon_is_one_error_after_the_line() {
    let parse = parse("let x = 1\nlet y = 2;");
    assert_eq!(
        problems(&parse.diagnostics),
        ["LM-P001 9..9 expected `;`, found `let`"]
    );
    assert_eq!(
        shape(&parse.syntax()),
        "(FILE (LET_STMT let (NAME x) = (LITERAL 1)) (LET_STMT let (NAME y) = (LITERAL 2) ;))"
    );
}

#[test]
fn an_error_on_the_same_line_points_at_the_token() {
    assert_eq!(
        errors_of("let x = 1 let y = 2;"),
        ["LM-P001 10..13 expected `;`, found `let`"]
    );
}

#[test]
fn messages_list_everything_that_could_come_next() {
    assert_eq!(
        errors_of("f(a b);"),
        ["LM-P001 4..5 expected `,` or `)`, found `b`"]
    );
    // The grammar takes a further `;` after a statement, so it is expected too.
    assert_eq!(
        errors_of("{ 1;"),
        ["LM-P001 4..4 expected `;` or `}`, found end of file"]
    );
}

#[test]
fn a_missing_value_is_one_error() {
    let parse = parse("let x = ;\nlet y = 2;");
    assert_eq!(
        problems(&parse.diagnostics),
        ["LM-P001 8..9 expected an expression, found `;`"]
    );
    assert_eq!(
        shape(&parse.syntax()),
        "(FILE (LET_STMT let (NAME x) = ;) (LET_STMT let (NAME y) = (LITERAL 2) ;))"
    );
}

#[test]
fn a_missing_name_is_one_error() {
    assert_eq!(
        errors_of("let = 1;"),
        ["LM-P001 4..5 expected identifier, found `=`"]
    );
}

#[test]
fn a_keyword_used_as_a_name_is_one_error() {
    for text in ["let let;", "let fn = 1;", "let let = 1;\nlet y = 2;"] {
        let found = errors_of(text);
        assert_eq!(found.len(), 1, "{text:?} {found:?}");
    }
    assert_eq!(
        errors_of("let let;"),
        ["LM-P002 4..7 `let` is a keyword and can't be used as a name"]
    );
    assert_eq!(
        shape_of("let let;"),
        "(FILE (LET_STMT let (NAME (ERROR let)) ;))"
    );
}

#[test]
fn a_keyword_on_the_next_line_is_not_taken_as_the_name() {
    let parse = parse("let\nlet y = 2;");
    assert_eq!(
        problems(&parse.diagnostics),
        ["LM-P001 3..3 expected identifier, found `let`"]
    );
    assert!(shape(&parse.syntax()).contains("(LET_STMT let (NAME y)"));
}

#[test]
fn unknown_characters_are_reported_once() {
    assert_eq!(
        errors_of("let x = 1 $$ ;"),
        ["LM-L001 10..12 unexpected characters `$$`"]
    );
    assert_eq!(
        errors_of("let x = $ ;"),
        ["LM-L001 8..9 unexpected character `$`"]
    );
}

#[test]
fn unexpected_tokens_are_wrapped_until_the_parser_can_go_on() {
    let parse = parse(") ] , let x = 1;");
    assert_eq!(
        problems(&parse.diagnostics),
        ["LM-P001 0..1 expected a statement, found `)`"]
    );
    assert_eq!(
        shape(&parse.syntax()),
        "(FILE (ERROR ) ] ,) (LET_STMT let (NAME x) = (LITERAL 1) ;))"
    );
}

#[test]
fn separate_mistakes_are_separate_errors() {
    assert_eq!(errors_of("let = 1;\nlet y = ;").len(), 2);
}

#[test]
fn diagnostics_are_in_source_order() {
    let codes: Vec<_> = parse("let = 1;\nlet y = $;")
        .diagnostics
        .iter()
        .map(|d| d.code)
        .collect();
    assert_eq!(codes, ["LM-P001", "LM-L001"]);
}

fn all_tokens(node: &SyntaxNode) -> Vec<String> {
    node.children_with_tokens()
        .map(|child| match child {
            NodeOrToken::Node(node) => kind_name_of(node.kind()).to_owned(),
            NodeOrToken::Token(token) => token.text().to_owned(),
        })
        .collect()
}

#[test]
fn trivia_stays_outside_the_node_that_follows() {
    let root = parse("  // head\n  let x = 1; // tail\n").syntax();
    assert_eq!(
        all_tokens(&root),
        ["  ", "// head", "\n  ", "LET_STMT", " ", "// tail", "\n"]
    );
}

#[test]
fn documentation_comments_belong_to_the_node_that_follows() {
    let root = parse("1;\n// plain\n/// about x\n/// more\nlet x = 1;").syntax();
    let item = root
        .children()
        .find(|node| node.kind() == LET_STMT)
        .expect("a let statement");
    assert_eq!(
        all_tokens(&item)[..4],
        ["/// about x", "\n", "/// more", "\n"]
    );
}

#[test]
fn a_documentation_comment_at_the_top_belongs_to_the_first_node() {
    let root = parse("// licence\n/// about x\nlet x = 1;").syntax();
    assert_eq!(all_tokens(&root), ["// licence", "\n", "LET_STMT"]);
}

#[test]
fn contextual_keywords_are_found_where_asked_for() {
    fn grammar(p: &mut Parser<'_>) {
        let m = p.start();
        assert!(!p.at_contextual(LET_KW));
        assert!(p.at_contextual(FROM_KW));
        assert!(p.eat_contextual(FROM_KW));
        assert!(!p.eat_contextual(FROM_KW));
        p.bump();
        m.complete(p, FILE);
    }
    let root = parse_with("from here", None, grammar).syntax();
    let first = root.first_token().expect("a token");
    assert_eq!(first.kind(), FROM_KW);
}

#[test]
fn deep_nesting_is_a_diagnostic_not_a_crash() {
    let text = format!("{}1{};", "(".repeat(100_000), ")".repeat(100_000));
    assert_eq!(
        errors_of(&text),
        ["LM-P003 255..256 this is nested more than 256 levels deep"]
    );
}

#[test]
fn a_very_long_chain_is_a_diagnostic_not_a_crash() {
    let text = format!("1{};", "+1".repeat(200_000));
    let parse = parse(&text);
    let found = problems(&parse.diagnostics);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("LM-P003"), "{found:?}");
    assert!(
        found[0].ends_with("this is nested more than 1024 levels deep"),
        "{found:?}"
    );
    // Walking and dropping the tree must not overflow either.
    assert!(parse.syntax().descendants().count() > 1000);
}

#[test]
fn limits_can_be_changed() {
    let limits = ParseLimits::default().max_depth(4);
    let parse = parse_with("((((1))));", Some(limits), file);
    assert_eq!(parse.diagnostics.len(), 1);
    assert_eq!(parse.diagnostics[0].code, "LM-P003");
}

/// Looks at tokens without ever taking one.
fn stuck(p: &mut Parser<'_>) {
    let m = p.start();
    while !p.at_end() {
        let _ = p.at(SEMI);
    }
    m.complete(p, FILE);
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "a loop is not making progress")]
fn a_parser_that_stops_consuming_fails_an_assertion_in_debug_builds() {
    parse_with("a b c", None, stuck);
}

#[cfg(not(debug_assertions))]
#[test]
fn a_parser_that_stops_consuming_is_stopped() {
    let parse = parse_with("a b c", None, stuck);
    assert_eq!(
        problems(&parse.diagnostics),
        ["LM-P004 0..1 the parser stopped making progress here"]
    );
}

#[test]
fn a_stopped_parser_keeps_the_text() {
    // Past the depth limit the parser sees only the end, so the rest is kept as is.
    let limits = ParseLimits::default().max_depth(1);
    let parse = parse_with("a b c", Some(limits), |p| {
        let outer = p.start();
        let inner = p.start();
        p.bump();
        inner.complete(p, NAME);
        outer.complete(p, FILE);
    });
    assert_eq!(
        problems(&parse.diagnostics),
        ["LM-P003 0..1 this is nested more than 1 levels deep"]
    );
}

#[test]
fn tokens_the_parser_left_are_kept() {
    fn one(p: &mut Parser<'_>) {
        let m = p.start();
        p.bump();
        m.complete(p, FILE);
    }
    let parse = parse_with("a b c // end", None, one);
    assert_eq!(
        problems(&parse.diagnostics),
        ["LM-P001 2..3 expected end of file, found `b`"]
    );
    assert_eq!(shape(&parse.syntax()), "(FILE a (ERROR b c))");
}

#[test]
fn careless_markers_do_not_panic() {
    fn nothing(_: &mut Parser<'_>) {}
    fn unfinished(p: &mut Parser<'_>) {
        let _outer: Marker = p.start();
        p.bump();
        let _inner: Marker = p.start();
    }
    fn wrong_order(p: &mut Parser<'_>) {
        let outer = p.start();
        let inner = p.start();
        p.bump();
        outer.complete(p, FILE);
        let late = inner.complete(p, NAME);
        let again = late.precede(p);
        again.complete(p, NAME);
    }
    fn two_roots(p: &mut Parser<'_>) {
        for _ in 0..2 {
            let m = p.start();
            p.bump();
            m.complete(p, NAME);
        }
    }
    fn bump_past_the_end(p: &mut Parser<'_>) {
        let m = p.start();
        for _ in 0..10 {
            p.bump();
            p.bump_as(NAME);
            p.bump_glued(SHR, 5);
        }
        m.complete(p, FILE);
    }
    for grammar in [
        nothing,
        unfinished,
        wrong_order,
        two_roots,
        bump_past_the_end,
    ] {
        for text in ["", "a", "a b", " a // b "] {
            parse_with(text, None, grammar);
        }
    }
}

#[test]
fn the_tree_builder_can_be_driven_directly() {
    let lexer = c_like();
    let text = " a + b ";
    let lexed = lex(&lexer, text);
    let mut builder = TreeBuilder::new(text, &lexed.tokens);
    builder.start_node(FILE);
    builder.start_node(BINARY);
    for _ in 0..3 {
        let kind = NAME_REF;
        builder.token(kind, 1);
    }
    builder.finish_node();
    builder.finish_node();
    // Extra calls change nothing.
    builder.finish_node();
    builder.token(NAME_REF, 1);
    let root = SyntaxNode::new_root(builder.finish());
    assert_eq!(root.text().to_string(), text);
    assert_eq!(shape(&root), "(FILE (BINARY a + b))");

    let unused = TreeBuilder::new(text, &lexed.tokens).finish();
    let root = SyntaxNode::new_root(unused);
    assert_eq!(root.kind(), SyntaxKind::ERROR);
    assert_eq!(root.text().to_string(), text);
}

#[test]
fn any_short_text_can_be_parsed() {
    const PIECES: [&str; 14] = [
        "let", " ", "x", "=", "1", ";", "(", ")", "{", "}", "\n", "<", ">", "$",
    ];
    let lexer = c_like();
    for len in 0..=4u32 {
        for mut number in 0..PIECES.len().pow(len) {
            let mut text = String::new();
            for _ in 0..len {
                text.push_str(PIECES[number % PIECES.len()]);
                number /= PIECES.len();
            }
            let source = source(&text);
            let mut parser = Parser::new(&source, lex(&lexer, &text), &lexer);
            file(&mut parser);
            let parse = parser.finish();
            assert_eq!(parse.syntax().text().to_string(), text);
            assert!(
                parse.diagnostics.iter().all(|d| d.code != "LM-P004"),
                "{text:?} made no progress"
            );
        }
    }
}

#[test]
fn a_held_back_error_builds_no_message() {
    // `expected` and `recover` both name the missing thing, but only the first of
    // two errors at one place is reported.
    fn grammar(p: &mut Parser<'_>) {
        let m = p.start();
        p.expected("a first thing");
        p.expected("a second thing");
        p.recover("a third thing", &[SEMI]);
        p.error("LM-X001", "a fourth thing");
        p.error_expected();
        m.complete(p, FILE);
    }
    let parse = parse_with("a ;", None, grammar);
    assert_eq!(
        problems(&parse.diagnostics),
        ["LM-P001 0..1 expected a first thing, found `a`"]
    );
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "does not follow the text")]
fn a_token_outside_the_text_fails_an_assertion_in_debug_builds() {
    use langmeld_syntax::Lexeme;
    use langmeld_syntax::{TextRange, TextSize};
    let token = Lexeme {
        kind: NAME,
        range: TextRange::new(TextSize::new(0), TextSize::new(50)),
        trivia: false,
        joint: false,
    };
    let mut builder = TreeBuilder::new("abc", std::slice::from_ref(&token));
    builder.start_node(FILE);
    builder.token(NAME, 1);
}

#[cfg(not(debug_assertions))]
#[test]
fn a_token_outside_the_text_becomes_an_error_token_holding_the_rest() {
    use langmeld_syntax::Lexeme;
    use langmeld_syntax::{TextRange, TextSize};
    let tokens = [
        Lexeme {
            kind: NAME,
            range: TextRange::new(TextSize::new(0), TextSize::new(1)),
            trivia: false,
            joint: false,
        },
        Lexeme {
            kind: NAME,
            range: TextRange::new(TextSize::new(1), TextSize::new(50)),
            trivia: false,
            joint: false,
        },
    ];
    let mut builder = TreeBuilder::new("a😀bc", &tokens);
    builder.start_node(FILE);
    builder.token(NAME, 1);
    builder.token(NAME, 1);
    let root = SyntaxNode::new_root(builder.finish());
    assert_eq!(root.text().to_string(), "a😀bc");
    let last = root.last_token().expect("a token");
    assert_eq!(last.kind(), SyntaxKind::ERROR);
    assert_eq!(last.text(), "😀bc");
}
