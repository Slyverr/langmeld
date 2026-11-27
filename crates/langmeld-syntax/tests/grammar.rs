//! Languages declared with the builder and run by the grammar engine.

use langmeld_syntax::pattern::*;
use langmeld_syntax::{
    Assoc, Cardinality, Diagnostic, ExprSpec, ExternRule, FileId, IdentStyle, IntPreset, Language,
    LanguageBuilder, Parse, ParseLimits, Parser, Preset, Severity, SourceFile, StringPreset,
    SyntaxKind, SyntaxNode, Token, choice, seq,
};
use rowan::NodeOrToken;

fn basics(name: &str) -> LanguageBuilder {
    let mut string = StringPreset::default();
    string.double = true;
    Language::builder(name)
        .token(Token::skip("ws", r"[ \t\r\n]+"))
        .token(Token::preset("ident", Preset::Ident(IdentStyle::Xid)))
        .token(Token::preset("int", Preset::Int(IntPreset::default())))
        .token(Token::preset("string", Preset::String(string)))
        .line_comment("//")
}

/// A problem with a grammar as one line: code, message and notes.
fn say(problem: &Diagnostic) -> String {
    format!(
        "{} {} [{}]",
        problem.code,
        problem.message,
        problem.notes.join(" | ")
    )
}

fn build(builder: LanguageBuilder) -> Language {
    match builder.build() {
        Ok(language) => language,
        Err(problems) => {
            let problems: Vec<_> = problems.iter().map(say).collect();
            panic!("the grammar is refused:\n{}", problems.join("\n"));
        }
    }
}

fn refused(builder: LanguageBuilder) -> Vec<String> {
    match builder.build() {
        Ok(_) => Vec::new(),
        Err(problems) => problems.iter().map(say).collect(),
    }
}

fn parse(language: &Language, text: &str) -> Parse {
    let file = SourceFile::new(FileId::new(0), "test", text).expect("small");
    let parse = langmeld_syntax::parse(language, &file);
    assert_eq!(
        parse.syntax().text().to_string(),
        text,
        "the tree is lossless"
    );
    for diagnostic in &parse.diagnostics {
        assert!(
            usize::from(diagnostic.range.end()) <= text.len(),
            "{diagnostic:?}"
        );
    }
    let broken = language
        .schema()
        .check(&parse.syntax(), |kind| language.kind_name(kind).to_owned());
    if parse.diagnostics.is_empty() {
        assert!(broken.is_empty(), "{text:?} breaks the schema: {broken:?}");
    }
    parse
}

/// The tree on one line, without whitespace and comments.
fn shape(language: &Language, node: &SyntaxNode) -> String {
    let mut out = format!("({}", language.kind_name(node.kind()));
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(node) => {
                out.push(' ');
                out.push_str(&shape(language, &node));
            }
            NodeOrToken::Token(token) => {
                let text = token.text();
                if !text.trim().is_empty() && !text.starts_with("//") {
                    out.push(' ');
                    out.push_str(text);
                }
            }
        }
    }
    out.push(')');
    out
}

fn tree(language: &Language, text: &str) -> String {
    let parse = parse(language, text);
    assert_eq!(problems(&parse), Vec::<String>::new(), "{text:?}");
    shape(language, &parse.syntax())
}

fn problems(parse: &Parse) -> Vec<String> {
    parse
        .diagnostics
        .iter()
        .map(|d| {
            let (start, end) = (u32::from(d.range.start()), u32::from(d.range.end()));
            format!("{} {start}..{end} {}", d.code, d.message)
        })
        .collect()
}

fn errors(language: &Language, text: &str) -> Vec<String> {
    problems(&parse(language, text))
}

fn lets() -> Language {
    build(
        basics("Lets")
            .rule("File", many(r("Stmt")))
            .rule(
                "Stmt",
                seq![
                    lit("let"),
                    field("name", r("Name")),
                    opt(seq![lit(":"), field("type", r("Name"))]),
                    lit("="),
                    field("value", r("Value")),
                    lit(";")
                ],
            )
            .rule("Name", r("ident"))
            .rule("Value", choice![r("int"), r("string"), r("Name")])
            .describe("Stmt", "a statement"),
    )
}

#[test]
fn rules_become_nodes() {
    let language = lets();
    assert_eq!(
        tree(&language, "let a = 1; let b: t = a;"),
        "(File (Stmt let (Name a) = (Value 1) ;) \
         (Stmt let (Name b) : (Name t) = (Value (Name a)) ;))"
    );
    assert_eq!(tree(&language, ""), "(File)");
}

#[test]
fn words_in_rules_are_reserved() {
    let language = lets();
    assert_eq!(
        errors(&language, "let let = 1;"),
        ["LM-P002 4..7 `let` is a keyword and can't be used as a name"]
    );
}

#[test]
fn contextual_words_stay_names_elsewhere() {
    let language = build(
        basics("Moves")
            .contextual(["from"])
            .rule("File", many(r("Move")))
            .rule("Move", seq![lit("move"), r("Name"), lit("from"), r("Name")])
            .rule("Name", r("ident")),
    );
    assert_eq!(
        tree(&language, "move from from from"),
        "(File (Move move (Name from) from (Name from)))"
    );
    assert_eq!(
        errors(&language, "move a to b"),
        ["LM-P001 7..9 expected `from`, found `to`"]
    );
}

#[test]
fn a_contextual_word_ends_a_list_and_stops_recovery() {
    let language = build(
        basics("Items")
            .contextual(["use", "let"])
            .rule("File", many(r("Item")))
            .rule("Item", choice![r("Use"), r("Let")])
            .rule("Use", seq![lit("use"), many1(r("ident"))])
            .rule("Let", seq![lit("let"), r("ident")]),
    );
    // `let` could be one more name, but here it starts the next item.
    assert_eq!(
        tree(&language, "use a b let c use d"),
        "(File (Use use a b) (Let let c) (Use use d))"
    );
    // Skipping unexpected text stops at the word too.
    let parse = parse(&language, "use a 1 2 let c");
    assert_eq!(problems(&parse), ["LM-P001 6..7 expected ident, found `1`"]);
    assert!(
        shape(&language, &parse.syntax()).ends_with("(Let let c))"),
        "{}",
        shape(&language, &parse.syntax())
    );
}

#[test]
fn the_first_rule_is_the_root_whatever_comes_before_it() {
    let language = build(
        basics("Root")
            .expr(ExprSpec::new("Expr").atom("Var", r("ident")))
            .extern_rule(ExternRule::new("Mark", |p| p.bump()).first(["!"]))
            .rule("File", seq![opt(r("Mark")), r("Expr")]),
    );
    assert_eq!(tree(&language, "a"), "(File (Var a))");
}

#[test]
fn a_contextual_word_is_still_a_name_where_the_word_means_nothing() {
    let language = build(
        basics("Words")
            .contextual(["show"])
            .rule("File", many(r("Stmt")))
            .rule("Stmt", choice![r("Show"), r("Set"), r("Edge")])
            .rule("Show", seq![lit("show"), r("ident")])
            .rule("Set", seq![r("Key"), lit(":"), r("int")])
            .rule("Key", choice![r("ident"), lit("show")])
            .rule("Edge", seq![r("ident"), lit("->"), r("ident")]),
    );
    assert_eq!(
        tree(&language, "show a show: 1 show -> show show show"),
        "(File (Show show a) (Set (Key show) : 1) (Edge show -> show) (Show show show))"
    );
}

#[test]
fn a_missing_token_is_one_error() {
    let language = lets();
    assert_eq!(
        errors(&language, "let a = 1\nlet b = 2;"),
        ["LM-P001 9..9 expected `;`, found `let`"]
    );
    assert_eq!(
        errors(&language, "let a 1;"),
        ["LM-P001 6..7 expected `:` or `=`, found `1`"]
    );
    assert_eq!(
        errors(&language, "let a = ;"),
        ["LM-P001 8..9 expected a value, found `;`"]
    );
}

#[test]
fn unexpected_text_is_skipped_up_to_the_next_item() {
    let language = lets();
    // Text that is no token is reported by the lexer alone.
    assert_eq!(
        errors(&language, ") = 3 let a = 1;"),
        ["LM-L001 0..1 unexpected character `)`"]
    );
    assert_eq!(
        errors(&language, "= 3 ; let a = 1;"),
        ["LM-P001 0..1 expected a statement, found `=`"]
    );
}

fn items() -> Language {
    build(
        basics("Items")
            .rule("File", many(r("Item")))
            .rule("Item", choice![r("Prop"), r("Edge"), r("Call"), r("Block")])
            .rule("Prop", seq![r("ident"), lit(":"), r("int")])
            .rule("Edge", seq![r("ident"), lit("->"), r("ident")])
            .rule(
                "Call",
                seq![r("ident"), lit("("), sep(r("int"), ","), lit(")")],
            )
            .rule("Block", seq![lit("{"), many(r("Item")), lit("}")])
            .describe("Item", "an item"),
    )
}

#[test]
fn a_choice_of_rules_has_no_node_and_looks_further_ahead() {
    let language = items();
    assert_eq!(
        tree(&language, "a: 1 a -> b f(1, 2,) { g() }"),
        "(File (Prop a : 1) (Edge a -> b) (Call f ( 1 , 2 , )) \
         (Block { (Call g ( )) }))"
    );
}

#[test]
fn lists_report_a_missing_separator_and_go_on() {
    let language = items();
    assert_eq!(
        errors(&language, "f(1 2) a: 1"),
        ["LM-P001 4..5 expected `,` or `)`, found `2`"]
    );
}

#[test]
fn unexpected_text_inside_a_list_is_skipped_there() {
    let language = items();
    let parse = parse(&language, "f(: -> 2) a: 1");
    assert_eq!(problems(&parse), ["LM-P001 2..3 expected int, found `:`"]);
    assert_eq!(
        shape(&language, &parse.syntax()),
        "(File (Call f ( (Error : ->) 2 )) (Prop a : 1))"
    );
}

#[test]
fn an_unclosed_block_is_one_error() {
    let language = items();
    assert_eq!(
        errors(&language, "{ a: 1"),
        ["LM-P001 6..6 expected `}`, found end of file"]
    );
    // A stray closer at the top level is skipped.
    assert_eq!(
        errors(&language, "} a: 1"),
        ["LM-P001 0..1 expected an item, found `}`"]
    );
}

#[test]
fn a_strict_list_has_no_trailing_separator() {
    let language =
        build(basics("Strict").rule("File", seq![lit("["), sep_strict(r("int"), ","), lit("]")]));
    assert_eq!(tree(&language, "[1, 2]"), "(File [ 1 , 2 ])");
    assert_eq!(
        errors(&language, "[1, 2,]"),
        ["LM-P001 6..7 expected int, found `]`"]
    );
}

#[test]
fn separators_and_terminators_may_be_joined_operators() {
    // With `>` in use, `>>` is read as two tokens and joined where it is meant.
    let lists = build(basics("Lists").rule("File", seq![lit("<"), sep(r("int"), ">>"), lit(">")]));
    assert_eq!(tree(&lists, "<1 >> 2>>3 >"), "(File < 1 >> 2 >> 3 >)");
    assert_eq!(errors(&lists, "<1 > > 2>").len(), 1);
    let ends = build(
        basics("Ends")
            .terminators([">>"])
            .rule("File", many(terminated(r("Item"))))
            .rule("Item", seq![lit("<"), r("int"), lit(">")]),
    );
    assert_eq!(
        tree(&ends, "<1> >> <2>"),
        "(File (Item < 1 >) >> (Item < 2 >))"
    );
    // A separator that cannot be told apart from what follows is explained.
    let refused = refused(basics("Clash").rule(
        "File",
        seq![sep(r("int"), ">>"), lit(">>"), r("int"), lit(">")],
    ));
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert!(refused[0].starts_with("LM-G004 in `File`"), "{refused:?}");
}

#[test]
fn custom_code_may_start_with_a_joined_operator() {
    let language = build(
        basics("Starts")
            .rule("File", many(choice![r("Shifted"), r("Plain")]))
            .rule("Plain", seq![r("int"), opt(lit("!"))])
            .extern_rule(
                ExternRule::new("Shifted", |p| {
                    let node = p.start();
                    let shift = p.kind_of(">>");
                    p.bump_glued(shift, 2);
                    node.complete(p, p.kind_of("Shift"));
                })
                .first([">>"])
                // With `>` in use too, `>>` is two tokens.
                .uses([">"])
                .produces(["Shift"]),
            ),
    );
    assert_eq!(tree(&language, ">> 1"), "(File (Shift >>) (Plain 1))");
}

#[test]
fn many1_needs_one() {
    let language = build(basics("Some").rule("File", many1(r("int"))));
    assert_eq!(tree(&language, "1 2"), "(File 1 2)");
    assert_eq!(
        errors(&language, ""),
        ["LM-P001 0..0 expected int, found end of file"]
    );
}

#[test]
fn lookahead_decides_without_taking() {
    let language = build(
        basics("Ends")
            .rule("File", many(r("Block")))
            .rule("Block", seq![lit("{"), many(r("Stmt")), lit("}")])
            .rule("Stmt", seq![r("int"), choice![lit(";"), look(lit("}"))]]),
    );
    assert_eq!(
        tree(&language, "{ 1; 2 }"),
        "(File (Block { (Stmt 1 ;) (Stmt 2) }))"
    );
    assert_eq!(
        errors(&language, "{ 1 2 }"),
        ["LM-P001 4..5 expected `;`, found `2`"]
    );
}

#[test]
fn negative_lookahead_keeps_a_rule_out() {
    let language = build(
        basics("Not")
            .rule("File", many(choice![r("Label"), r("Word")]))
            .rule("Label", seq![r("ident"), lit(":")])
            .rule("Word", seq![not(seq![r("ident"), lit(":")]), r("ident")]),
    );
    assert_eq!(tree(&language, "a: b"), "(File (Label a :) (Word b))");
}

fn sums() -> Language {
    build(
        basics("Sums")
            .rule("File", many(r("Stmt")))
            .rule("Stmt", seq![r("Expr"), lit(";")])
            .rule("NameRef", r("ident"))
            .expr(
                ExprSpec::new("Expr")
                    .atom("Literal", r("int"))
                    .atom("Paren", seq![lit("("), field("inner", r("Expr")), lit(")")])
                    .atom("Var", r("NameRef"))
                    .prefix("Neg", ["-", "!"], 9)
                    .infix("Pow", ["**"], 8, Assoc::Right)
                    .infix("Mul", ["*", "/"], 6, Assoc::Left)
                    .infix("Add", ["+", "-"], 5, Assoc::Left)
                    .infix("Cmp", ["==", "<", ">", ">="], 4, Assoc::None)
                    .infix("Shift", [">>"], 4, Assoc::None)
                    .infix("Assign", ["="], 1, Assoc::Right)
                    .postfix(
                        "Call",
                        seq![lit("("), field("args", sep(r("Expr"), ",")), lit(")")],
                        10,
                    )
                    .postfix("Try", lit("?"), 10),
            )
            .describe("Stmt", "a statement"),
    )
}

#[test]
fn operators_follow_their_precedence_and_grouping() {
    let language = sums();
    assert_eq!(
        tree(&language, "1+2*3-4;"),
        "(File (Stmt (Add (Add (Literal 1) + (Mul (Literal 2) * (Literal 3))) - \
         (Literal 4)) ;))"
    );
    assert_eq!(
        tree(&language, "a = b = 2 ** 3 ** 4;"),
        "(File (Stmt (Assign (Var (NameRef a)) = (Assign (Var (NameRef b)) = \
         (Pow (Literal 2) ** (Pow (Literal 3) ** (Literal 4))))) ;))"
    );
}

#[test]
fn prefix_and_postfix_operators() {
    let language = sums();
    assert_eq!(
        tree(&language, "x*-!1;"),
        "(File (Stmt (Mul (Var (NameRef x)) * (Neg - (Neg ! (Literal 1)))) ;))"
    );
    assert_eq!(
        tree(&language, "-f(1)(a, 2)?;"),
        "(File (Stmt (Neg - (Try (Call (Call (Var (NameRef f)) ( (Literal 1) )) \
         ( (Var (NameRef a)) , (Literal 2) )) ?)) ;))"
    );
    assert_eq!(
        tree(&language, "(1+2)*3;"),
        "(File (Stmt (Mul (Paren ( (Add (Literal 1) + (Literal 2)) )) * (Literal 3)) ;))"
    );
}

#[test]
fn operators_made_of_closing_angles_are_joined_by_the_parser() {
    let language = sums();
    assert_eq!(
        tree(&language, "a >> 1;"),
        "(File (Stmt (Shift (Var (NameRef a)) >> (Literal 1)) ;))"
    );
    assert_eq!(
        tree(&language, "a >= 1;"),
        "(File (Stmt (Cmp (Var (NameRef a)) >= (Literal 1)) ;))"
    );
    // Apart, they are not one operator.
    assert_eq!(
        errors(&language, "a > > 1;"),
        ["LM-P001 4..5 expected an expression, found `>`"]
    );
}

#[test]
fn chaining_operators_that_do_not_chain_is_one_error() {
    let language = sums();
    assert_eq!(
        errors(&language, "a < b < c;"),
        ["LM-P006 6..7 `<` cannot be chained; add parentheses around one of the operations"]
    );
}

#[test]
fn a_missing_operand_is_one_error() {
    let language = sums();
    assert_eq!(
        errors(&language, "1 + ;\n2;"),
        ["LM-P001 4..5 expected an expression, found `;`"]
    );
    assert_eq!(
        errors(&language, "f(1 2);"),
        ["LM-P001 4..5 expected `,` or `)`, found `2`"]
    );
}

#[test]
fn generics_split_what_expressions_join() {
    let language = build(
        basics("Generics")
            .rule("File", many(r("Stmt")))
            .rule("Stmt", seq![r("Type"), lit(";")])
            .rule(
                "Type",
                seq![
                    r("ident"),
                    opt(seq![lit("<"), sep(r("Type"), ","), lit(">")])
                ],
            )
            .rule("Shift", seq![r("ident"), lit(">>"), r("ident")])
            .rule("Unused", r("Shift")),
    );
    assert_eq!(
        tree(&language, "Vec<Vec<T>>;"),
        "(File (Stmt (Type Vec < (Type Vec < (Type T) >) >) ;))"
    );
}

fn lines() -> Language {
    build(
        Language::builder("Lines")
            .token(Token::skip("ws", r"[ \t\r\n]+"))
            .token(Token::preset("ident", Preset::Ident(IdentStyle::Ascii)))
            .token(Token::preset("int", Preset::Int(IntPreset::default())))
            .terminators([";"])
            .rule("File", many(terminated(r("Stmt"))))
            .rule("Stmt", choice![r("Set"), r("Group")])
            .rule("Set", seq![r("ident"), lit("="), r("Expr")])
            .rule(
                "Group",
                seq![
                    lit("group"),
                    lit("{"),
                    many(terminated(r("Stmt"))),
                    lit("}")
                ],
            )
            .expr(
                ExprSpec::new("Expr")
                    .atom("Literal", r("int"))
                    .atom("List", seq![lit("["), sep(r("Expr"), ","), lit("]")])
                    .infix("Add", ["+"], 5, Assoc::Left),
            ),
    )
}

#[test]
fn newlines_end_statements() {
    let language = build(
        Language::builder("Lines")
            .token(Token::skip("ws", r"[ \t\r\n]+"))
            .token(Token::preset("ident", Preset::Ident(IdentStyle::Ascii)))
            .token(Token::preset("int", Preset::Int(IntPreset::default())))
            .line_comment("//")
            .terminators([";"])
            .rule("File", many(terminated(r("Set"))))
            .rule("Set", seq![r("ident"), lit("="), r("int")]),
    );
    assert_eq!(
        tree(&language, "\n// head\na = 1\nb = 2; c = 3\r\n\r\nd = 4"),
        "(File (Set a = 1) (Set b = 2) ; (Set c = 3) (Set d = 4))"
    );
    assert_eq!(
        errors(&language, "a = 1 b = 2"),
        ["LM-P001 6..7 expected end of line or `;`, found `b`"]
    );
}

#[test]
fn newlines_continue_inside_brackets_and_after_operators() {
    let language = lines();
    assert_eq!(
        tree(
            &language,
            "a = [\n1,\n2\n]\nb = 1 +\n2\ngroup {\n  c = 3\n  d = 4 }\n"
        ),
        "(File (Set a = (List [ (Literal 1) , (Literal 2) ])) \
         (Set b = (Add (Literal 1) + (Literal 2))) \
         (Group group { (Set c = (Literal 3)) (Set d = (Literal 4)) }))"
    );
}

#[test]
fn the_brackets_that_continue_lines_can_be_chosen() {
    let angle = |chosen: bool| {
        let builder = basics("Angles")
            .terminators([";"])
            .rule("File", many(terminated(r("Item"))))
            .rule("Item", seq![lit("<"), many(r("int")), lit(">")]);
        build(if chosen {
            builder.continue_inside([("<", ">")])
        } else {
            builder
        })
    };
    let text = "<1\n2>\n<3>";
    assert_eq!(
        tree(&angle(true), text),
        "(File (Item < 1 2 >) (Item < 3 >))"
    );
    // By default only parentheses and square brackets continue lines.
    assert!(!errors(&angle(false), text).is_empty());
}

#[test]
fn a_rule_can_say_where_it_recovers() {
    let language = |recover: bool| {
        let builder = basics("Recover")
            .rule("File", many(r("Stmt")))
            .rule("Stmt", seq![lit("{"), many(r("int")), lit("}")])
            .describe("Stmt", "a statement");
        build(if recover {
            builder.recover("Stmt", ["!"])
        } else {
            builder
        })
    };
    // Without help, the junk and the `}` are skipped up to the next statement.
    assert_eq!(errors(&language(false), "{ 1 x y } { 2 }").len(), 1);
    let with = language(true);
    let parse = parse(&with, "{ 1 x y ! 2 } { 3 }");
    assert_eq!(problems(&parse), ["LM-P001 4..5 expected int, found `x`"]);
    // Skipping stopped at the `!`.
    assert!(shape(&with, &parse.syntax()).contains("(Error x y)"));
}

#[test]
fn attempt_tries_one_way_then_another() {
    let language = build(
        basics("Try")
            .rule("File", many(choice![attempt(r("Arrow")), r("Group")]))
            .rule(
                "Arrow",
                seq![lit("("), many(r("ident")), lit(")"), lit("=>"), r("int")],
            )
            .rule("Group", seq![lit("("), many(r("ident")), lit(")")]),
    );
    assert_eq!(
        tree(&language, "(a b c d) => 1 (a b c d)"),
        "(File (Arrow ( a b c d ) => 1) (Group ( a b c d )))"
    );
}

/// `( names ) => value` or `( value )`, which needs unbounded lookahead.
fn arrow_or_paren(p: &mut Parser<'_>) {
    let node = p.start();
    let is_arrow = p.lookahead(|p| {
        let open = p.kind_of("(");
        p.expect(open);
        while !p.at_end() && p.current_text() != ")" {
            p.bump();
        }
        p.bump();
        let arrow = p.kind_of("=>");
        p.expect(arrow);
    });
    p.bump();
    if is_arrow {
        while !p.at_end() && p.current_text() != ")" {
            p.rule("Name");
            let comma = p.kind_of(",");
            p.eat(comma);
        }
        p.bump();
        p.bump();
        p.rule("Expr");
        let kind = p.kind_of("Arrow");
        node.complete(p, kind);
    } else {
        p.rule("Expr");
        let close = p.kind_of(")");
        p.expect(close);
        let kind = p.kind_of("Paren");
        node.complete(p, kind);
    }
}

#[test]
fn an_extern_rule_handles_what_lookahead_cannot() {
    let language = build(
        basics("Arrows")
            .rule("File", many(seq![r("Expr"), lit(";")]))
            .rule("Name", r("ident"))
            .extern_rule(
                ExternRule::new("ArrowOrParen", arrow_or_paren)
                    .first(["("])
                    .uses([")", "=>", ","])
                    .produces(["Arrow", "Paren"]),
            )
            .expr(
                ExprSpec::new("Expr")
                    .atom("Var", r("Name"))
                    .atom("Group", r("ArrowOrParen"))
                    .infix("Add", ["+"], 5, Assoc::Left),
            ),
    );
    assert_eq!(
        tree(&language, "(a, b) => a + b; (a + b);"),
        "(File (Group (Arrow ( (Name a) , (Name b) ) => (Add (Var (Name a)) + \
         (Var (Name b))))) ; (Group (Paren ( (Add (Var (Name a)) + (Var (Name b))) ))) ;)"
    );
}

#[test]
fn debug_builds_report_nodes_that_are_not_declared() {
    let language = build(
        basics("Odd")
            .rule("File", r("Custom"))
            .extern_rule(ExternRule::new("Custom", |p| {
                let node = p.start();
                p.bump();
                // No node of the language has this kind.
                node.complete(p, SyntaxKind(999));
            })),
    );
    let file = SourceFile::new(FileId::new(0), "test", "x").expect("small");
    let parsed = langmeld_syntax::parse(&language, &file);
    let expected: &[&str] = if cfg!(debug_assertions) {
        &[
            "LM-S001 0..1 the grammar's code made a wrong tree: `File` cannot have `unknown` as a child",
        ]
    } else {
        &[]
    };
    assert_eq!(problems(&parsed), expected);
}

#[test]
fn doc_comments_and_block_comments_are_declared_by_name() {
    let language = build(
        basics("Docs")
            .doc_comment("///")
            .nested_block_comment("/*", "*/")
            .rule("File", many(r("Item")))
            .rule("Item", seq![lit("item"), r("ident")]),
    );
    let parse = parse(&language, "/* /* x */ */\n/// about a\nitem a");
    let item = parse.syntax().first_child().expect("an item");
    assert!(item.text().to_string().starts_with("/// about a"));
}

#[test]
fn tokens_can_be_described_like_rules() {
    let language = build(
        basics("Named")
            .rule("File", r("ident"))
            .describe("ident", "a name"),
    );
    assert_eq!(
        errors(&language, "1"),
        ["LM-P001 0..1 expected a name, found `1`"]
    );
}

#[test]
fn the_schema_describes_the_nodes() {
    let language = sums();
    let schema = language.schema();
    let call = schema.node("Call").expect("a postfix node");
    let args = call.field("args").expect("a labelled part");
    assert_eq!(args.cardinality, Cardinality::Many);
    assert_eq!(args.nodes, ["Expr"]);
    let paren = schema.node("Paren").expect("an atom node");
    assert_eq!(
        paren.field("inner").expect("labelled").cardinality,
        Cardinality::One
    );
    assert_eq!(
        schema.union("Expr").expect("an expression is a union"),
        [
            "Literal", "Paren", "Var", "Neg", "Pow", "Mul", "Add", "Cmp", "Shift", "Assign",
            "Call", "Try"
        ]
    );

    let lets = lets();
    let stmt = lets.schema().node("Stmt").expect("a rule node");
    assert_eq!(
        stmt.field("type").expect("labelled").cardinality,
        Cardinality::Optional
    );
    assert_eq!(stmt.field("value").expect("labelled").nodes, ["Value"]);
    assert!(lets.schema().node("Missing").is_none());
}

// Refused grammars: every check has a case that triggers it and a near miss that does
// not, and says what is wrong in plain words with a way to fix it.

#[test]
fn undefined_names_suggest_the_closest_one() {
    assert_eq!(
        refused(
            basics("G")
                .rule("File", many(r("NameRfe")))
                .rule("NameRef", r("ident"))
        ),
        [
            "LM-G001 `File` uses `NameRfe`, which is not a rule or a token \
             [To fix this, did you mean `NameRef`? If not, declare `NameRfe`.]",
            "LM-G006 `NameRef` is never used [To fix this, use it, or remove it.]",
        ]
    );
    assert_eq!(
        refused(basics("G").rule("File", r("zzzzzz"))),
        [
            "LM-G001 `File` uses `zzzzzz`, which is not a rule or a token \
             [To fix this, declare `zzzzzz` as a rule or a token.]"
        ]
    );
}

#[test]
fn names_that_are_declared_are_accepted() {
    // Near miss for LM-G001: rules, expression nodes' rules and tokens all resolve.
    build(
        basics("G")
            .rule("File", many(seq![r("NameRef"), r("int")]))
            .rule("NameRef", r("ident")),
    );
}

#[test]
fn an_unknown_root_or_recover_rule_is_undefined_too() {
    assert_eq!(
        refused(basics("G").rule("File", r("int")).root("Main")),
        [
            "LM-G001 `Main` is set as the root rule, but nothing is declared with that name \
             [To fix this, declare `Main` as a rule or a token.]"
        ]
    );
    assert_eq!(
        refused(
            basics("G")
                .rule("File", many(r("int")))
                .recover("Fil", ["!"])
        ),
        ["LM-G001 `recover` names `Fil`, which is not a rule \
             [To fix this, did you mean `File`? If not, declare `Fil`.]"]
    );
}

#[test]
fn left_recursion_is_explained() {
    assert_eq!(
        refused(
            basics("G")
                .rule("File", r("Sum"))
                .rule("Sum", choice![seq![r("Term"), lit("+")], r("int")])
                .rule("Term", r("Sum"))
        ),
        [
            "LM-G002 `Sum` can start with itself (Sum -> Term -> Sum), so parsing it would \
             never take a token [In technical terms: this is left recursion, which a parser \
             that reads from left to right cannot handle. | To fix this, move operators into \
             an `expr` block.]"
        ]
    );
}

#[test]
fn recursion_after_a_token_is_fine() {
    // Near miss for LM-G002: a token comes first, so every round takes one.
    let language = build(basics("G").rule("File", r("List")).rule(
        "List",
        choice![seq![lit("("), r("List"), lit(")")], r("int")],
    ));
    assert_eq!(
        tree(&language, "((1))"),
        "(File (List ( (List ( (List 1) )) )))"
    );
}

#[test]
fn an_attempt_that_may_take_nothing_cannot_hide_recursion() {
    // A failed attempt is passed over, so `A` can start with itself.
    assert_eq!(
        refused(basics("G").rule("A", seq![attempt(lit("x")), r("A")])),
        [
            "LM-G002 `A` can start with itself (A -> A), so parsing it would never take a \
             token [In technical terms: this is left recursion, which a parser that reads \
             from left to right cannot handle. | To fix this, move operators into an `expr` \
             block.]"
        ]
    );
}

#[test]
fn choices_that_need_more_than_three_tokens_are_refused() {
    let alternatives = |third: &'static str| {
        refused(
            basics("G")
                .rule("File", choice![r("A"), r("B")])
                .rule("A", seq![lit("("), lit("("), lit("("), r("int"), lit(")")])
                .rule(
                    "B",
                    seq![lit("("), lit("("), lit(third), r("ident"), lit(")")],
                ),
        )
    };
    assert_eq!(
        alternatives("("),
        [
            "LM-G003 in `File`, `A` and `B` both start with `(` `(` `(`, and 3 tokens are not \
             enough to tell them apart [In technical terms: the parser decides by looking at \
             most 3 tokens ahead (it is LL(3)), and both choices have the same first 3 tokens. \
             | To fix this, factor out the common start, or use `try` / `extern rule`.]"
        ]
    );
    // Near miss: the third token tells them apart.
    assert_eq!(alternatives("["), Vec::<String>::new());
}

#[test]
fn three_tokens_of_lookahead_are_enough_here() {
    let language = build(
        basics("Three")
            .rule("File", many(choice![r("A"), r("B")]))
            .rule("A", seq![lit("("), lit("("), r("int"), lit(")"), lit(")")])
            .rule(
                "B",
                seq![lit("("), lit("("), r("ident"), lit(")"), lit(")")],
            ),
    );
    assert_eq!(
        tree(&language, "((1)) ((a))"),
        "(File (A ( ( 1 ) )) (B ( ( a ) )))"
    );
}

/// The language of the running example, with the block written as given.
fn calc(block: Pattern, statement_end: Pattern) -> LanguageBuilder {
    basics("Calc")
        .keywords(["let", "fn", "return", "if", "else", "true", "false"])
        .rule("File", field("items", many(r("Item"))))
        .rule("Item", choice![r("LetStmt"), r("FnDecl"), r("ExprStmt")])
        .rule(
            "LetStmt",
            seq![
                lit("let"),
                field("name", r("Name")),
                lit("="),
                field("value", r("Expr")),
                lit(";")
            ],
        )
        .rule(
            "FnDecl",
            seq![
                lit("fn"),
                field("name", r("Name")),
                lit("("),
                field("params", sep(r("Param"), ",")),
                lit(")"),
                field("body", r("Block"))
            ],
        )
        .rule("Param", field("name", r("Name")))
        .rule("Block", block)
        .rule("ExprStmt", seq![field("expr", r("Expr")), statement_end])
        .rule("Name", r("ident"))
        .rule("NameRef", r("ident"))
        .expr(
            ExprSpec::new("Expr")
                .atom(
                    "Literal",
                    choice![r("int"), r("string"), lit("true"), lit("false")],
                )
                .atom("Paren", seq![lit("("), field("inner", r("Expr")), lit(")")])
                .atom("Var", r("NameRef"))
                .atom(
                    "If",
                    seq![
                        lit("if"),
                        field("cond", r("Expr")),
                        field("then", r("Block")),
                        opt(seq![lit("else"), field("else", r("Block"))])
                    ],
                )
                .prefix("Neg", ["-"], 9)
                .infix("Mul", ["*", "/"], 6, Assoc::Left)
                .infix("Add", ["+", "-"], 5, Assoc::Left)
                .infix("Cmp", ["==", "<"], 4, Assoc::None)
                .postfix(
                    "Call",
                    seq![lit("("), field("args", sep(r("Expr"), ",")), lit(")")],
                    10,
                ),
        )
}

#[test]
fn the_ambiguous_block_is_refused_with_the_fix() {
    // `Block = "{" Item* Expr? "}"`: the last expression could be one more item.
    let ambiguous = refused(calc(
        seq![lit("{"), many(r("Item")), opt(r("Expr")), lit("}")],
        choice![lit(";"), look(lit("}"))],
    ));
    assert_eq!(
        ambiguous,
        [
            "LM-G003 in `Block`, the repeated `Item` and what follows it both start with `Expr`, \
             so the parser cannot tell where the repetition ends [In technical terms: the \
             choice between going on and stopping is made by looking at most 3 tokens ahead \
             (LL(3)), and both ways can begin with `Expr`. | To fix this, factor out the \
             common start, for example by ending the repeated part with `;` or with a \
             lookahead such as `&\"}\"`, or use `try` / `extern rule`.]"
        ]
    );
}

#[test]
fn the_fixed_block_parses() {
    // Near miss: the last expression ends with `&"}"`, so a block is `{ Item* }`.
    let language = build(calc(
        seq![lit("{"), field("stmts", many(r("Item"))), lit("}")],
        choice![lit(";"), look(lit("}"))],
    ));
    assert_eq!(
        tree(&language, "fn add(a, b) { let c = a + b; c * 2 }"),
        "(File (FnDecl fn (Name add) ( (Param (Name a)) , (Param (Name b)) ) (Block { \
         (LetStmt let (Name c) = (Add (Var (NameRef a)) + (Var (NameRef b))) ;) \
         (ExprStmt (Mul (Var (NameRef c)) * (Literal 2))) })))"
    );
}

#[test]
fn a_repetition_that_collides_with_what_follows_is_refused() {
    // Looking two tokens ahead tells the last `int` apart.
    build(basics("G").rule("File", seq![many(r("int")), r("int")]));
    // With three after the repetition, no look ahead can.
    assert_eq!(
        refused(basics("G").rule("File", seq![many(r("int")), r("int"), r("int"), r("int")])),
        [
            "LM-G004 in `File`, after the repeated `int`, `int` `int` `int` could either \
             continue the repetition or belong to what follows it, and the parser cannot tell \
             which [In technical terms: the first 3 tokens of the part and of what follows it \
             overlap (their FIRST and FOLLOW sets collide). | To fix this, make the repeated \
             part start differently from what follows it.]"
        ]
    );
    assert_eq!(
        refused(basics("G").rule("File", seq![opt(lit(";")), lit(";"), lit(";"), lit(";")])),
        [
            "LM-G004 in `File`, after the optional `;`, `;` `;` `;` could either start the \
             optional part or belong to what follows it, and the parser cannot tell which \
             [In technical terms: the first 3 tokens of the part and of what follows it overlap \
             (their FIRST and FOLLOW sets collide). | To fix this, make the optional part start \
             differently from what follows it.]"
        ]
    );
}

#[test]
fn a_list_that_collides_with_what_follows_is_refused() {
    // After the list, `, int , int` could be more of it, as far as three tokens show.
    let problems = refused(basics("G").rule(
        "File",
        seq![sep(r("int"), ","), lit(","), r("int"), lit(","), r("int")],
    ));
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].starts_with("LM-G004 in `File`, after the list of `int`"),
        "{problems:?}"
    );
    // Near miss: what follows starts differently.
    build(basics("G").rule("File", seq![sep(r("int"), ","), lit(";")]));
}

#[test]
fn a_repetition_of_nothing_is_refused() {
    assert_eq!(
        refused(basics("G").rule("File", many(opt(r("int"))))),
        [
            "LM-G005 in `File`, a repeated part can match nothing, so the repetition could go on \
             without end [To fix this, require at least one token inside the repetition.]"
        ]
    );
    // Near miss: the repeated part needs a token.
    build(basics("G").rule("File", many(seq![lit("!"), opt(r("int"))])));
}

#[test]
fn an_unused_rule_is_a_warning() {
    let language = build(basics("G").rule("File", r("int")).rule("Spare", r("ident")));
    let warnings: Vec<_> = language.warnings().iter().map(say).collect();
    assert_eq!(
        warnings,
        ["LM-G006 `Spare` is never used [To fix this, use it, or remove it.]"]
    );
    assert_eq!(language.warnings()[0].severity, Severity::Warning);
}

#[test]
fn a_grammar_that_uses_every_rule_has_no_warnings() {
    let language = build(
        basics("G")
            .rule("File", many(r("Item")))
            .rule("Item", r("int")),
    );
    assert!(language.warnings().is_empty());
}

#[test]
fn custom_code_may_call_any_rule_so_none_is_unused() {
    let language = build(
        basics("G")
            .rule("File", r("Hook"))
            .rule("Elsewhere", r("ident"))
            .extern_rule(
                ExternRule::new("Hook", |p| {
                    p.rule("Elsewhere");
                })
                .first(["ident"]),
            ),
    );
    assert!(language.warnings().is_empty());
}

#[test]
fn a_custom_rule_without_first_tokens_is_a_warning() {
    let language = build(
        basics("G")
            .rule("File", r("Hook"))
            .extern_rule(ExternRule::new("Hook", |p| p.bump())),
    );
    let warnings: Vec<_> = language.warnings().iter().map(say).collect();
    assert_eq!(
        warnings,
        [
            "LM-G015 the custom rule `Hook` says nothing about how it starts, so it may start \
             with anything [To fix this, list the literals or tokens it can start with using \
             `first`, so that choices around it can be checked.]"
        ]
    );
    assert_eq!(language.warnings()[0].severity, Severity::Warning);
    // The grammar still parses.
    assert!(parse(&language, "a").diagnostics.is_empty());
}

#[test]
fn a_custom_rule_with_first_tokens_is_not_warned_about() {
    let language = build(
        basics("G")
            .rule("File", r("Hook"))
            .extern_rule(ExternRule::new("Hook", |p| p.bump()).first(["ident"])),
    );
    assert!(language.warnings().is_empty());
}

#[test]
fn a_keyword_that_no_identifier_matches_is_refused() {
    let refusal = refused(
        Language::builder("G")
            .token(Token::skip("ws", r"[ \t\r\n]+"))
            .token(Token::regex("word", "[a-z]+").identifier())
            .keywords(["Let"])
            .rule("File", seq![lit("Let"), r("word")]),
    );
    assert_eq!(refusal.len(), 1, "{refusal:?}");
    assert!(
        refusal[0].starts_with("LM-G008 keyword `Let` is not matched as an identifier"),
        "{refusal:?}"
    );
}

#[test]
fn a_keyword_that_an_identifier_matches_is_accepted() {
    build(
        basics("G")
            .keywords(["let"])
            .rule("File", seq![lit("let"), r("ident")]),
    );
}

#[test]
fn a_token_with_the_same_text_as_another_can_never_match() {
    // `//` starts a line comment, so the literal `//` can never be matched.
    let refusal = refused(basics("G").rule("File", many(lit("//"))));
    assert_eq!(refusal.len(), 1, "{refusal:?}");
    assert!(refusal[0].starts_with("LM-G009 token `"), "{refusal:?}");
    assert!(refusal[0].contains("can never match"), "{refusal:?}");
    // Near miss: a different text is fine.
    build(basics("G").rule("File", many(lit("/"))));
}

#[test]
fn two_string_tokens_that_read_the_same_quotes_shadow_each_other() {
    let mut double = StringPreset::default();
    double.double = true;
    let refusal = refused(
        Language::builder("G")
            .token(Token::skip("ws", r"[ \t\r\n]+"))
            .token(Token::preset("a", Preset::String(double.clone())))
            .token(Token::preset("b", Preset::String(double)))
            .rule("File", many(r("a"))),
    );
    assert!(
        refusal
            .iter()
            .any(|p| p.starts_with("LM-G009 token `b` can never match")),
        "{refusal:?}"
    );
}

#[test]
fn token_problems_use_the_same_form() {
    // LM-G007 comes from the lexer, in the same form as the grammar's own.
    assert_eq!(
        refused(
            basics("G")
                .token(Token::regex("empty", "x*"))
                .rule("File", r("empty"))
        ),
        [
            "LM-G007 token `empty` can match empty text [To fix this, require at least one \
             character, for example with `+` instead of `*`.]"
        ]
    );
    build(
        basics("G")
            .token(Token::regex("some", "x+"))
            .rule("File", r("some")),
    );
}

#[test]
fn ambiguous_operators_are_refused() {
    assert_eq!(
        refused(
            basics("G").rule("File", r("Expr")).expr(
                ExprSpec::new("Expr")
                    .atom("Literal", r("int"))
                    .infix("Add", ["+"], 5, Assoc::Left)
                    .infix("Concat", ["+"], 4, Assoc::Left),
            )
        ),
        [
            "LM-G010 in `Expr`, `+` continues an expression as both `Add` and `Concat` \
             [To fix this, give the operators different symbols.]"
        ]
    );
    assert_eq!(
        refused(
            basics("G").rule("File", seq![r("Expr"), lit("!")]).expr(
                ExprSpec::new("Expr")
                    .atom("Literal", r("int"))
                    .postfix("Assert", lit("!"), 9),
            )
        ),
        [
            "LM-G010 in `Expr`, after an expression, `!` could continue it as `Assert` or \
             belong to what follows it [To fix this, use a different symbol for the operator, \
             or for what follows.]"
        ]
    );
}

#[test]
fn operators_with_different_symbols_are_fine() {
    // Near miss for LM-G010.
    build(
        basics("G").rule("File", seq![r("Expr"), lit(";")]).expr(
            ExprSpec::new("Expr")
                .atom("Literal", r("int"))
                .infix("Add", ["+"], 5, Assoc::Left)
                .infix("Concat", ["++"], 4, Assoc::Left)
                .postfix("Assert", lit("!"), 9),
        ),
    );
}

#[test]
fn names_are_declared_once() {
    assert_eq!(
        refused(basics("G").rule("File", r("int")).rule("File", r("ident"))),
        ["LM-G011 `File` is declared twice [To fix this, rename one of the two.]"]
    );
    // A node of an expression counts as a name too.
    let twice = refused(
        basics("G")
            .rule("File", r("Expr"))
            .rule("Literal", r("int"))
            .expr(ExprSpec::new("Expr").atom("Literal", r("int"))),
    );
    assert_eq!(
        twice[0],
        "LM-G011 `Literal` is declared twice [To fix this, rename one of the two.]"
    );
    build(basics("G").rule("File", r("Item")).rule("Item", r("int")));
}

#[test]
fn a_word_cannot_be_both_reserved_and_contextual() {
    let errors = refused(
        basics("Words")
            .keywords(["from", "to"])
            .contextual(["from"])
            .rule("File", many(r("int"))),
    );
    assert_eq!(
        errors,
        ["LM-G011 `from` is declared both reserved and contextual \
             [To fix this, keep one of the two.]"]
    );
    build(
        basics("Words")
            .keywords(["to"])
            .contextual(["from"])
            .rule("File", many(r("int"))),
    );
}

#[test]
fn a_language_has_rules() {
    assert_eq!(
        refused(basics("G")),
        ["LM-G012 the language has no rules [To fix this, declare at least one rule.]"]
    );
    build(basics("G").rule("File", r("int")));
}

#[test]
fn postfix_operators_start_with_a_symbol() {
    let postfix = |body: Pattern| {
        refused(
            Language::builder("T")
                .token(Token::preset("int", Preset::Int(IntPreset::default())))
                .rule("File", many(r("Expr")))
                .expr(
                    ExprSpec::new("Expr")
                        .atom("Literal", r("int"))
                        .postfix("Index", body, 2),
                ),
        )
    };
    assert_eq!(
        postfix(r("int")),
        [
            "LM-G013 in `Expr`, the postfix operator `Index` does not start with a literal \
             [To fix this, start it with the symbol that marks the operator.]"
        ]
    );
    assert_eq!(
        postfix(seq![lit("["), r("int"), lit("]")]),
        Vec::<String>::new()
    );
}

#[test]
fn a_language_has_a_limited_number_of_kinds() {
    let words = |count: usize| {
        refused(
            Language::builder("T")
                .token(Token::preset("ident", Preset::Ident(IdentStyle::Ascii)))
                .rule("File", choice((0..count).map(|n| lit(format!("w{n}"))))),
        )
    };
    assert_eq!(
        words(60_001),
        ["LM-G014 the language has more than 60000 kinds \
             [To fix this, use fewer tokens and rules.]"]
    );
    assert_eq!(words(3_000), Vec::<String>::new());
}

#[test]
fn lookahead_that_would_take_too_much_is_refused() {
    // Many statements that share their starts and can be inside one another.
    let large = |statements: usize| {
        let mut builder = basics("Large")
            .rule("File", many(r("Item")))
            .rule("Item", choice((0..statements).map(|n| r(format!("S{n}")))));
        for n in 0..statements {
            builder = builder.rule(
                format!("S{n}"),
                seq![
                    lit(format!("a{}", n % 40)),
                    r("ident"),
                    r("ident"),
                    many(r(format!("Tail{}", n % 7))),
                    lit(format!("e{n}"))
                ],
            );
        }
        for n in 0..7 {
            builder = builder.rule(
                format!("Tail{n}"),
                seq![lit(format!("t{n}")), opt(r("ident")), opt(r("Item"))],
            );
        }
        refused(builder)
    };
    let found = large(80);
    assert!(!found.is_empty());
    let costly = found
        .iter()
        .find(|problem| problem.starts_with("LM-G016 in `"))
        .unwrap_or_else(|| panic!("{:?}", &found[..found.len().min(5)]));
    assert!(
        costly.contains("a choice needs to look 2 tokens ahead")
            || costly.contains("a choice needs to look 3 tokens ahead"),
        "{costly}"
    );
    assert!(
        costly
            .ends_with("[To fix this, factor out the common start, or use `try` / `extern rule`.]")
    );
    // Near miss: a few statements are checked in full.
    let small = large(3);
    assert!(
        small.iter().all(|problem| !problem.starts_with("LM-G016")),
        "{small:?}"
    );
}

#[test]
fn every_problem_is_reported_at_once() {
    let errors =
        refused(basics("G").rule("File", seq![r("Nope"), many(opt(r("int"))), r("Other")]));
    assert_eq!(errors.len(), 3, "{errors:?}");
}

#[test]
fn any_short_text_can_be_parsed() {
    const PIECES: [&str; 14] = [
        "a", "1", "(", ")", "{", "}", "=", ";", ",", "+", "group", "[", "\n", ">",
    ];
    let languages = [sums(), lines(), items()];
    for len in 0..=4u32 {
        for mut number in 0..PIECES.len().pow(len) {
            let mut text = String::new();
            for _ in 0..len {
                text.push_str(PIECES[number % PIECES.len()]);
                text.push(' ');
                number /= PIECES.len();
            }
            for language in &languages {
                let found = parse(language, &text).diagnostics;
                assert!(
                    found.iter().all(|d| d.code != "LM-P004"),
                    "{language:?} stalls on {text:?}"
                );
            }
        }
    }
}

#[test]
fn deep_input_is_a_diagnostic() {
    let language = sums();
    for text in [
        format!("{}1{};", "(".repeat(10_000), ")".repeat(10_000)),
        format!("{}1;", "-".repeat(10_000)),
        format!("1{};", "+1".repeat(50_000)),
        format!("1{};", "**1".repeat(10_000)),
    ] {
        let found = parse(&language, &text).diagnostics;
        assert!(found.iter().any(|d| d.code == "LM-P003"), "{found:?}");
    }
}

#[test]
fn line_breaks_that_end_nothing_are_passed_over() {
    let language = build(
        basics("Lines")
            .terminators([";"])
            .rule("File", many(r("Item")))
            .rule(
                "Item",
                seq![
                    lit("item"),
                    opt(r("int")),
                    lit("{"),
                    many(r("ident")),
                    lit("}"),
                    many(r("Prop"))
                ],
            )
            .rule(
                "Prop",
                terminated(seq![lit("prop"), many(r("int")), opt(r("Inner"))]),
            )
            .rule("Inner", seq![lit("{"), many(r("ident")), lit("}")]),
    );
    // Between items, inside braces and before an optional part there is nothing a
    // line could end.
    let text = "\nitem\n1\n{\na\nb\n}\nprop 1 2\nprop 3 {\nc\nd\n}\n\nitem {}\n";
    assert_eq!(errors(&language, text), Vec::<String>::new());
    assert_eq!(
        tree(&language, "item { a }\nitem {\n}\n"),
        "(File (Item item { a }) (Item item { }))"
    );
    // An operator may start the next line where no statement is being read.
    let sums = build(
        basics("Free sums")
            .terminators([";"])
            .rule("File", many(seq![r("Expr"), lit("!")]))
            .expr(ExprSpec::new("Expr").atom("Literal", r("int")).infix(
                "Add",
                ["+"],
                1,
                Assoc::Left,
            )),
    );
    assert_eq!(errors(&sums, "1\n+ 2\n!\n3 !"), Vec::<String>::new());
    // Inside a statement the end of its line still ends it.
    let parse = parse(&language, "item {}\nprop 1\n2\n");
    assert_eq!(problems(&parse).len(), 1, "{:?}", problems(&parse));
}

#[test]
fn braces_inside_braces_keep_count_of_statements() {
    let language = build(
        basics("Nested")
            .terminators([";"])
            .rule("File", many(r("Stmt")))
            .rule(
                "Stmt",
                terminated(seq![
                    lit("{"),
                    lit("{"),
                    many(r("ident")),
                    lit("}"),
                    many(r("int")),
                    lit("}"),
                    opt(r("string"))
                ]),
            ),
    );
    let text = "{ {\na\n}\n1\n} \"x\"\n{{}}\n";
    assert_eq!(errors(&language, text), Vec::<String>::new());
    // After the braces, the end of the line ends the statement again.
    assert_eq!(errors(&language, "{{}}\n\"x\"\n").len(), 1);
}

// Where parsing resumes after text that nothing can use.

/// Calls, blocks and properties, which nest.
fn nested() -> Language {
    build(
        basics("Nested")
            .rule("File", many(r("Item")))
            .rule("Item", choice![r("Call"), r("Block"), r("Prop")])
            .rule(
                "Call",
                seq![r("ident"), lit("("), sep(r("int"), ","), lit(")")],
            )
            .rule("Block", seq![lit("{"), many(r("Item")), lit("}")])
            .rule("Prop", seq![lit("let"), r("ident"), lit(":"), r("int")])
            .describe("Item", "an item"),
    )
}

#[test]
fn text_in_a_list_is_skipped_up_to_what_can_go_on() {
    let language = nested();
    let parse = parse(&language, "f(1 : : 2) g(3)");
    assert_eq!(problems(&parse), ["LM-P001 4..5 expected int, found `:`"]);
    // Parsing resumed at the `2`, which the list can go on with.
    assert_eq!(
        shape(&language, &parse.syntax()),
        "(File (Call f ( 1 (Error : :) 2 )) (Call g ( 3 )))"
    );
}

#[test]
fn a_missing_closer_resumes_at_what_an_enclosing_rule_expects() {
    let language = nested();
    // `)` is missing, so the call ends where the next item starts; the block still
    // closes, and the file goes on after it: one mistake, one diagnostic.
    let parse = parse(&language, "{ f(1 : g(3) } h(4)");
    assert_eq!(problems(&parse), ["LM-P001 6..7 expected int, found `:`"]);
    assert_eq!(
        shape(&language, &parse.syntax()),
        "(File (Block { (Call f ( 1 (Error :)) (Call g ( 3 )) }) (Call h ( 4 )))"
    );
}

#[test]
fn text_between_items_is_skipped_up_to_the_next_item() {
    let language = nested();
    let parse = parse(&language, "f(1) ) : 3 { g(2) }");
    assert_eq!(
        problems(&parse),
        ["LM-P001 5..6 expected an item, found `)`"]
    );
    assert_eq!(
        shape(&language, &parse.syntax()),
        "(File (Call f ( 1 )) (Error ) : 3) (Block { (Call g ( 2 )) }))"
    );
}

#[test]
fn skipping_inside_a_block_stops_at_its_closer() {
    let language = nested();
    // `}` closes the block, though a loop around it could only resume at an item.
    let parse = parse(&language, "{ : 1 } f(1)");
    assert_eq!(
        problems(&parse),
        ["LM-P001 2..3 expected an item, found `:`"]
    );
    assert_eq!(
        shape(&language, &parse.syntax()),
        "(File (Block { (Error : 1) }) (Call f ( 1 )))"
    );
}

#[test]
fn an_unclosed_block_is_reported_once_at_the_end() {
    let language = nested();
    assert_eq!(
        errors(&language, "{ f(1) { g(2)"),
        ["LM-P001 13..13 expected `}`, found end of file"]
    );
}

#[test]
fn separate_mistakes_are_separate_diagnostics() {
    let language = nested();
    assert_eq!(
        errors(&language, "f(1 : 2) g(3 3) ) h(4)"),
        [
            "LM-P001 4..5 expected int, found `:`",
            "LM-P001 13..14 expected `,` or `)`, found `3`",
            "LM-P001 16..17 expected an item, found `)`",
        ]
    );
}

#[test]
fn a_mistake_deep_inside_gives_one_diagnostic() {
    let language = nested();
    // The first `:` and everything that follows it up to a place to resume.
    assert_eq!(
        errors(&language, "{ { { f(1 : : : 2) } } } g(1)"),
        ["LM-P001 10..11 expected int, found `:`"]
    );
    assert_eq!(
        errors(&language, "{ { { let x 1 } } } g(1)"),
        ["LM-P001 12..13 expected `:`, found `1`"]
    );
}

#[test]
fn recovery_does_not_depend_on_how_many_rules_there_are() {
    // The same mistake, in a grammar with a few rules and one with hundreds.
    let build_with = |extra: usize| {
        let mut builder = basics("Many")
            .rule("File", many(r("Item")))
            .rule("Item", choice![r("Call"), r("Block"), r("Prop")])
            .rule(
                "Call",
                seq![r("ident"), lit("("), sep(r("int"), ","), lit(")")],
            )
            .rule("Block", seq![lit("{"), many(r("Item")), lit("}")])
            .rule("Prop", seq![lit("let"), r("ident"), lit(":"), r("int")]);
        for n in 0..extra {
            builder = builder.rule(format!("Unused{n}"), seq![lit(format!("w{n}")), r("ident")]);
        }
        build(builder)
    };
    let text = "{ f(1 : : 2) g(3) } h(4)";
    let few = errors(&build_with(0), text);
    let many = errors(&build_with(300), text);
    assert_eq!(few, ["LM-P001 6..7 expected int, found `:`"]);
    assert_eq!(few, many);
}

// Custom code.

#[test]
fn an_extern_rule_calls_back_into_declared_rules_and_recovers_there() {
    // `Pair` takes two names, each read by the rule `Name`.
    let language = build(
        basics("Pairs")
            .rule("File", many(r("ReadPair")))
            .rule("Name", r("ident"))
            .extern_rule(
                ExternRule::new("ReadPair", |p| {
                    let node = p.start();
                    p.rule("Name");
                    let arrow = p.kind_of("->");
                    p.expect(arrow);
                    p.rule("Name");
                    let kind = p.kind_of("Pair");
                    node.complete(p, kind);
                })
                .first(["ident"])
                .uses(["->"])
                .produces(["Pair"]),
            ),
    );
    assert_eq!(
        tree(&language, "a -> b c -> d"),
        "(File (Pair (Name a) -> (Name b)) (Pair (Name c) -> (Name d)))"
    );
    // The mistake inside the callback is reported once, and the next pair is read.
    assert_eq!(
        errors(&language, "a b c -> d"),
        ["LM-P001 2..3 expected `->`, found `b`"]
    );
    assert_eq!(
        errors(&language, "a -> b c ->"),
        ["LM-P001 11..11 expected ident, found end of file"]
    );
}

#[test]
fn custom_code_that_calls_the_rule_that_called_it_stops_at_once() {
    // The engine is the same one all along, so it knows `File` is under way here: no
    // second `File` is started, and the depth limit is not what ends it.
    let language = build(
        basics("Round")
            .rule("File", r("Hook"))
            .extern_rule(ExternRule::new("Hook", |p| {
                // Nothing is parsed: `File` is under way at this very place.
                p.rule("File");
                p.bump();
            })),
    );
    assert_eq!(errors(&language, "x"), Vec::<String>::new());
}

#[test]
fn custom_code_that_calls_itself_is_stopped() {
    let language = build(
        basics("Loop")
            .rule("File", r("Again"))
            .extern_rule(ExternRule::new("Again", |p| {
                p.rule("Again");
            })),
    );
    let found = parse(&language, "x").diagnostics;
    assert_eq!(found[0].code, "LM-P003", "{found:?}");
}

#[test]
fn custom_code_that_nests_deeper_than_the_limit_is_stopped_where_it_is() {
    // Each level reads one parenthesis and calls the rule again.
    let language = build(
        basics("Parens")
            .parse_limits(ParseLimits::default().max_depth(16))
            .rule("File", r("Group"))
            .extern_rule(
                ExternRule::new("Group", |p| {
                    let node = p.start();
                    p.bump();
                    if p.at(p.kind_of("(")) {
                        p.rule("Group");
                    }
                    node.complete(p, SyntaxKind::ERROR);
                })
                .first(["("]),
            )
            .rule("Spare", lit(")")),
    );
    let found = parse(&language, &"(".repeat(100)).diagnostics;
    assert!(found.iter().any(|d| d.code == "LM-P003"), "{found:?}");
}

/// Calls the rule `Scan` for each token; it tries to read a whole list of numbers to
/// the end, over and over.
fn scanning() -> Language {
    build(
        basics("Scans").rule("File", many(r("Scan"))).extern_rule(
            ExternRule::new("Scan", |p| {
                let semi = p.kind_of(";");
                let int = p.kind_of("int");
                let read = p.attempt(|p| {
                    while p.eat(int) {}
                    p.expect(semi);
                });
                if !read {
                    p.bump();
                }
            })
            .first(["int"])
            .uses([";"]),
        ),
    )
}

#[test]
fn trying_again_and_again_is_stopped_with_a_diagnostic() {
    let language = scanning();
    let text = "1 ".repeat(20_000);
    let found = parse(&language, &text).diagnostics;
    assert!(
        found.iter().any(|d| d.code == "LM-P007"),
        "{:?}",
        found.first()
    );
    // Near miss: reading to a `;` takes the whole list once.
    assert_eq!(errors(&language, "1 2 3 ; 4 ;"), Vec::<String>::new());
}

// Limits.

#[test]
fn the_depth_limit_can_be_set_by_the_grammar() {
    let language = build(
        basics("Shallow")
            .parse_limits(ParseLimits::default().max_depth(4))
            .rule("File", r("Group"))
            .rule("Group", seq![lit("("), opt(r("Group")), lit(")")]),
    );
    assert_eq!(errors(&language, "((()))"), Vec::<String>::new());
    let found = errors(&language, "((((((()))))))");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].ends_with("this is nested more than 4 levels deep"),
        "{found:?}"
    );
}

// Dispatch and operators.

#[test]
fn a_choice_is_decided_by_one_look_whatever_the_number_of_ways() {
    // Each way starts with its own word; with 5 ways or 300, the next token alone
    // picks the one to take.
    let ways = |count: usize| {
        let mut builder = basics("Ways")
            .rule("File", many(r("Item")))
            .rule("Item", choice((0..count).map(|n| r(format!("W{n}")))));
        for n in 0..count {
            builder = builder.rule(format!("W{n}"), seq![lit(format!("w{n}")), r("int")]);
        }
        build(builder)
    };
    for count in [5, 300] {
        let language = ways(count);
        let last = format!("w{} 1", count - 1);
        assert_eq!(
            tree(&language, &format!("w0 1 {last}")),
            format!("(File (W0 w0 1) (W{} w{} 1))", count - 1, count - 1)
        );
        assert_eq!(
            errors(&language, "w1 w2 5"),
            ["LM-P001 3..5 expected int, found `w2`"]
        );
    }
}

#[test]
fn a_chain_of_operators_that_do_not_chain_is_a_plain_error() {
    let language = build(
        basics("Compare").rule("File", r("Expr")).expr(
            ExprSpec::new("Expr")
                .atom("Literal", r("int"))
                .infix("Less", ["<"], 4, Assoc::None)
                .infix("Same", ["=="], 4, Assoc::None),
        ),
    );
    assert_eq!(errors(&language, "1 < 2"), Vec::<String>::new());
    assert_eq!(
        errors(&language, "1 < 2 == 3"),
        ["LM-P006 6..8 `==` cannot be chained; add parentheses around one of the operations"]
    );
}

#[test]
fn operators_with_other_precedence_chain_with_one_that_does_not() {
    let language = sums();
    assert_eq!(errors(&language, "a < b + 1;"), Vec::<String>::new());
}

#[test]
fn lines_end_only_where_the_lexer_says_blocks_start_and_end() {
    // The braces are the lexer's block pair: inside them, a line ending ends only the
    // statements written there, wherever the rule that reads them is declared.
    let language = build(
        basics("Blocks")
            .terminators([";"])
            .rule("File", many(terminated(r("Stmt"))))
            .rule("Stmt", choice![r("Group"), r("Plain")])
            .rule("Plain", seq![lit("let"), r("ident"), opt(r("Body"))])
            .rule("Group", seq![lit("group"), r("Body")])
            .rule("Body", seq![lit("{"), many(r("ident")), lit("}")]),
    );
    assert_eq!(
        errors(&language, "group {\n a\n b\n}\nlet x {\n}\nlet y\n"),
        Vec::<String>::new()
    );
}

#[test]
fn a_missing_closer_is_one_error_when_what_comes_after_it_follows() {
    let language = build(
        basics("G")
            .rule("File", many(r("Fn")))
            .rule(
                "Fn",
                seq![
                    lit("fn"),
                    lit("("),
                    sep(r("Param"), ","),
                    lit(")"),
                    lit("{"),
                    lit("}")
                ],
            )
            .rule("Param", r("ident")),
    );
    let parse = parse(&language, "fn (a, b { } fn (c) { }");
    assert_eq!(
        problems(&parse),
        ["LM-P001 9..10 expected `,` or `)`, found `{`"]
    );
    // Near miss: text that is not what follows is still skipped, inside the list.
    let parse = self::parse(&language, "fn (a ? b) { }");
    assert_eq!(parse.diagnostics.len(), 1, "{:?}", parse.diagnostics);
}

#[test]
fn a_rule_can_be_called_again_and_again_from_custom_code() {
    // Calls that finished do not count towards the depth limit.
    let language = build(
        basics("Many")
            .rule("File", r("Names"))
            .rule("Name", r("ident"))
            .extern_rule(
                ExternRule::new("Names", |p| {
                    let node = p.start();
                    while !p.at_end() {
                        p.rule("Name");
                    }
                    let kind = p.kind_of("Pair");
                    node.complete(p, kind);
                })
                .first(["ident"])
                .produces(["Pair"]),
            ),
    );
    assert_eq!(errors(&language, &"a ".repeat(600)), Vec::<String>::new());
}

#[test]
fn the_root_is_not_checked_against_the_schema_when_the_text_has_errors() {
    let language = build(
        basics("Odd")
            .rule("File", seq![r("Custom"), many(r("ident"))])
            .extern_rule(ExternRule::new("Custom", |p| {
                let node = p.start();
                p.bump();
                node.complete(p, SyntaxKind(999));
            })),
    );
    // One mistake is reported, and the wrong tree is not on top of it.
    assert_eq!(
        errors(&language, "x 1"),
        ["LM-P001 2..3 expected ident, found `1`"]
    );
}

#[test]
fn a_wrong_tree_is_reported_where_it_is() {
    if !cfg!(debug_assertions) {
        return;
    }
    let language = build(
        basics("Odd")
            .rule("File", many(r("Item")))
            .rule("Item", seq![lit("item"), r("Custom")])
            .extern_rule(ExternRule::new("Custom", |p| {
                let node = p.start();
                p.bump();
                node.complete(p, SyntaxKind(999));
            })),
    );
    assert_eq!(
        errors(&language, "item a item b"),
        [
            "LM-S001 0..6 the grammar's code made a wrong tree: `Item` cannot have `unknown` as a child"
        ]
    );
}

#[test]
fn languages_and_lexers_can_be_printed() {
    let language = lets();
    let shown = format!("{language:?}");
    assert!(
        shown.starts_with("Language { name: \"Lets\", rules: [\"File\", \"Stmt\""),
        "{shown}"
    );
}

mod places {
    use langmeld_syntax::pattern::{lit, many, r};
    use langmeld_syntax::{
        FileId, IdentStyle, Language, Preset, SourceFile, StringPreset, TextRange, TextSize, Token,
        parse, seq,
    };

    fn range(start: u32, end: u32) -> TextRange {
        TextRange::new(TextSize::new(start), TextSize::new(end))
    }

    #[test]
    fn a_problem_points_where_its_rule_was_written() {
        let file = FileId::new(7);
        let problems = Language::builder("Placed")
            .token(Token::skip("space", r"\s+"))
            .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
            .rule("File", many(r("Item")))
            .locate("File", file, range(5, 9))
            .rule("Item", seq![r("word"), many(r("word")), r("word")])
            .locate("Item", file, range(20, 24))
            .build()
            .expect_err("the repetition collides with what follows it");
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(problems[0].code, "LM-G004");
        assert_eq!((problems[0].file, problems[0].range), (file, range(20, 24)));
    }

    #[test]
    fn an_unknown_name_points_at_its_own_pattern() {
        let file = FileId::new(3);
        let problems = Language::builder("Placed")
            .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
            .rule(
                "File",
                seq![lit("a"), r("Mising").located(file, range(30, 36))],
            )
            .locate("File", file, range(0, 4))
            .build()
            .expect_err("`Mising` is not declared");
        assert_eq!(problems[0].code, "LM-G001");
        assert_eq!((problems[0].file, problems[0].range), (file, range(30, 36)));
    }

    #[test]
    fn problems_without_a_place_point_at_the_language_header() {
        let file = FileId::new(1);
        let warnings = Language::builder("Placed")
            .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
            .rule("File", lit("a"))
            .rule("Spare", lit("b"))
            .locate("Placed", file, range(0, 6))
            .build()
            .expect("a warning only");
        assert_eq!(warnings.warnings()[0].code, "LM-G006");
        // The rule has no recorded place, so the header stands for it.
        assert_eq!(warnings.warnings()[0].range, range(0, 6));
    }

    #[test]
    fn interpolated_strings_hold_their_holes_as_expressions() {
        let mut quotes = StringPreset::default();
        quotes.double = true;
        let language = Language::builder("Holes")
            .token(Token::skip("space", r"\s+"))
            .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
            .token(Token::interpolated("string", quotes, "{", "}", "Name"))
            .rule("File", many(r("string")))
            .rule("Name", r("word"))
            .build()
            .expect("the grammar is valid");
        let source =
            SourceFile::new(FileId::new(0), "x", "\"a {b} c\" \"\" \"{d}{e}\"").expect("small");
        let parsed = parse(&language, &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.syntax().text(), source.text());
        let strings: Vec<_> = parsed.syntax().children().collect();
        assert_eq!(strings.len(), 3);
        assert_eq!(language.kind_name(strings[0].kind()), "string");
        let names = strings[2]
            .children()
            .filter(|node| language.kind_name(node.kind()) == "Name")
            .count();
        assert_eq!(names, 2);
        // A hole that is not an expression is one mistake.
        let broken = SourceFile::new(FileId::new(0), "x", "\"a {1} c\"").expect("small");
        assert_eq!(parse(&language, &broken).diagnostics.len(), 1);
    }

    #[test]
    fn lookahead_lists_the_choices_that_need_more_than_one_token() {
        let language = Language::builder("Ahead")
            .token(Token::skip("space", r"\s+"))
            .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
            .rule("File", many(r("Item")))
            .rule("Item", langmeld_syntax::choice![r("Call"), r("Pair")])
            .rule("Call", seq![r("word"), lit("(")])
            .rule("Pair", seq![r("word"), lit(":")])
            .build()
            .expect("the grammar is valid");
        let ahead = language.lookahead();
        assert_eq!(ahead.len(), 1, "{ahead:?}");
        assert_eq!(
            (ahead[0].rule.as_str(), ahead[0].starts.as_str()),
            ("Item", "word")
        );
        assert_eq!(ahead[0].tokens, 2);
        assert_eq!(language.max_lookahead(), 2);
    }
}
