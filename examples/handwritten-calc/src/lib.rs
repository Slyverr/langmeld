//! Calc, a small language parsed by hand on Langmeld's primitives.
//!
//! It shows what a parser of your own looks like: declare the tokens, then write one
//! function per rule. The tree stays complete whatever the source says, and one
//! mistake gives one diagnostic.
//!
//! ```text
//! file      = item*
//! item      = let | function | statement
//! let       = "let" name "=" expr ";"
//! function  = "fn" name "(" (name ("," name)* ","?)? ")" block
//! block     = "{" item* "}"
//! statement = expr ";"          the ";" may be left out before "}"
//! ```

use std::sync::OnceLock;

use langmeld_syntax::{
    CompletedMarker, FloatPreset, IdentStyle, IntPreset, Lexer, Parse, Parser, Preset, SourceFile,
    StringPreset, SyntaxKind, TokenSpec,
};

/// Declares one constant per kind, numbered in order, and their names.
macro_rules! kinds {
    ($($constant:ident $name:ident,)*) => {
        #[derive(Clone, Copy)]
        #[repr(u16)]
        enum Kind {
            $($name,)*
        }

        $(
            #[doc = concat!("The `", stringify!($name), "` kind.")]
            pub const $constant: SyntaxKind = SyntaxKind(Kind::$name as u16);
        )*

        /// The name of a token or node kind of Calc.
        pub fn kind_name(kind: SyntaxKind) -> &'static str {
            match kind {
                $($constant => stringify!($name),)*
                SyntaxKind::ERROR => "Error",
                _ => "Unknown",
            }
        }
    };
}

kinds! {
    WHITESPACE Whitespace,
    COMMENT Comment,
    IDENT Ident,
    INT Int,
    FLOAT Float,
    STRING String,
    LET_KW LetKw,
    FN_KW FnKw,
    RETURN_KW ReturnKw,
    IF_KW IfKw,
    ELSE_KW ElseKw,
    TRUE_KW TrueKw,
    FALSE_KW FalseKw,
    L_PAREN LParen,
    R_PAREN RParen,
    L_BRACE LBrace,
    R_BRACE RBrace,
    COMMA Comma,
    SEMI Semi,
    EQ Eq,
    EQ_EQ EqEq,
    LT Lt,
    PLUS Plus,
    MINUS Minus,
    STAR Star,
    SLASH Slash,
    FILE File,
    LET_STMT LetStmt,
    FN_DECL FnDecl,
    PARAM Param,
    BLOCK Block,
    EXPR_STMT ExprStmt,
    NAME Name,
    NAME_REF NameRef,
    LITERAL Literal,
    PAREN Paren,
    VAR Var,
    IF If,
    NEG Neg,
    MUL Mul,
    ADD Add,
    CMP Cmp,
    CALL Call,
}

/// What an item can start with, and so where parsing resumes after unexpected text.
const ITEM_START: &[SyntaxKind] = &[
    LET_KW, FN_KW, INT, FLOAT, STRING, TRUE_KW, FALSE_KW, L_PAREN, IF_KW, IDENT, MINUS,
];

/// What an expression can start with.
const EXPRESSION_START: &[SyntaxKind] = &[
    INT, FLOAT, STRING, TRUE_KW, FALSE_KW, L_PAREN, IF_KW, IDENT, MINUS,
];

/// Binding powers: a larger number binds tighter.
const COMPARISON: u8 = 4;
const SUM: u8 = 5;
const PRODUCT: u8 = 6;
const PREFIX: u8 = 9;
const CALL_POWER: u8 = 10;

fn build_lexer() -> Lexer {
    let mut int = IntPreset::default();
    int.hex = true;
    int.underscores = true;
    let mut float = FloatPreset::default();
    float.exponent = true;
    let mut string = StringPreset::default();
    string.double = true;

    let mut specs = vec![
        TokenSpec::regex(WHITESPACE, "whitespace", r"[ \t\r\n]+").skip(),
        TokenSpec::line_comment(COMMENT, "//"),
        TokenSpec::block_comment(COMMENT, "/*", "*/").nested(),
        TokenSpec::preset(IDENT, "identifier", Preset::Ident(IdentStyle::Xid)),
        TokenSpec::preset(INT, "integer", Preset::Int(int)),
        TokenSpec::preset(FLOAT, "float", Preset::Float(float)),
        TokenSpec::preset(STRING, "string", Preset::String(string)),
    ];
    let keywords = [
        (LET_KW, "let"),
        (FN_KW, "fn"),
        (RETURN_KW, "return"),
        (IF_KW, "if"),
        (ELSE_KW, "else"),
        (TRUE_KW, "true"),
        (FALSE_KW, "false"),
    ];
    specs.extend(keywords.map(|(kind, word)| TokenSpec::keyword(kind, word)));
    let punctuation = [
        (L_PAREN, "("),
        (R_PAREN, ")"),
        (L_BRACE, "{"),
        (R_BRACE, "}"),
        (COMMA, ","),
        (SEMI, ";"),
        (EQ, "="),
        (EQ_EQ, "=="),
        (LT, "<"),
        (PLUS, "+"),
        (MINUS, "-"),
        (STAR, "*"),
        (SLASH, "/"),
    ];
    specs.extend(punctuation.map(|(kind, text)| TokenSpec::punct(kind, text)));
    Lexer::builder()
        .tokens(specs)
        .build()
        .expect("the Calc tokens are fixed, and the tests build them")
}

/// The Calc lexer, built on first use and shared after that.
pub fn lexer() -> &'static Lexer {
    static LEXER: OnceLock<Lexer> = OnceLock::new();
    LEXER.get_or_init(build_lexer)
}

/// Parses a Calc file into a lossless tree and its diagnostics.
///
/// Any text can be parsed: a mistake gives a diagnostic and the tree still covers the
/// whole file.
///
/// ```
/// use langmeld_syntax::{FileId, SourceFile};
///
/// let file = SourceFile::new(FileId::new(0), "a.calc", "let x = 1 + 2;")?;
/// assert!(handwritten_calc::parse(&file).diagnostics.is_empty());
/// # Ok::<(), langmeld_syntax::TextTooLarge>(())
/// ```
pub fn parse(file: &SourceFile) -> Parse {
    let lexer = lexer();
    let mut parser = Parser::new(file, lexer.lex(file), lexer);
    source_file(&mut parser);
    parser.finish()
}

fn source_file(p: &mut Parser<'_>) {
    let m = p.start();
    while !p.at_end() {
        if ITEM_START.contains(&p.current()) {
            item(p);
        } else {
            p.recover("a statement", ITEM_START);
        }
    }
    m.complete(p, FILE);
}

fn item(p: &mut Parser<'_>) {
    match p.current() {
        LET_KW => let_statement(p),
        FN_KW => function(p),
        _ => expression_statement(p),
    }
}

fn let_statement(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    name(p);
    p.expect(EQ);
    expression(p);
    p.expect(SEMI);
    m.complete(p, LET_STMT);
}

fn function(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump();
    name(p);
    if p.expect(L_PAREN) {
        parameters(p);
        p.expect(R_PAREN);
    }
    block(p);
    m.complete(p, FN_DECL);
}

/// Parameters up to the `)`, with an optional comma after the last.
fn parameters(p: &mut Parser<'_>) {
    while !p.at(R_PAREN) && p.at(IDENT) {
        let m = p.start();
        name(p);
        m.complete(p, PARAM);
        if !p.eat(COMMA) && !p.at(R_PAREN) {
            p.error_expected();
        }
    }
}

fn block(p: &mut Parser<'_>) {
    let m = p.start();
    // Without its `{`, taking what follows as the body would only mislead.
    if p.expect(L_BRACE) {
        while !p.at(R_BRACE) && !p.at_end() {
            if ITEM_START.contains(&p.current()) {
                item(p);
            } else {
                let mut recovery = ITEM_START.to_vec();
                recovery.push(R_BRACE);
                p.recover("a statement", &recovery);
            }
        }
        p.expect(R_BRACE);
    }
    m.complete(p, BLOCK);
}

fn expression_statement(p: &mut Parser<'_>) {
    let m = p.start();
    expression(p);
    if !p.eat(SEMI) && p.current() != R_BRACE {
        p.error_expected();
    }
    m.complete(p, EXPR_STMT);
}

fn name(p: &mut Parser<'_>) {
    let m = p.start();
    p.expect(IDENT);
    m.complete(p, NAME);
}

fn expression(p: &mut Parser<'_>) {
    binary(p, 0);
}

/// Operators binding tighter than `min`, by precedence climbing.
fn binary(p: &mut Parser<'_>, min: u8) {
    let Some(mut left) = operand(p) else { return };
    let mut compared = false;
    loop {
        let (kind, power) = match p.current() {
            L_PAREN => (CALL, CALL_POWER),
            STAR | SLASH => (MUL, PRODUCT),
            PLUS | MINUS => (ADD, SUM),
            EQ_EQ | LT => (CMP, COMPARISON),
            _ => break,
        };
        if power <= min {
            break;
        }
        if kind == CMP {
            if compared {
                let message = format!(
                    "`{}` cannot follow another comparison; add parentheses around one of them",
                    p.current_text()
                );
                p.error("CALC-001", message);
            }
            compared = true;
        }
        let m = left.precede(p);
        if kind == CALL {
            arguments(p);
        } else {
            p.bump();
            binary(p, power);
        }
        left = m.complete(p, kind);
    }
}

/// An atom or a prefix operator with its operand.
fn operand(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    let m = p.start();
    let done = match p.current() {
        INT | FLOAT | STRING | TRUE_KW | FALSE_KW => {
            p.bump();
            m.complete(p, LITERAL)
        }
        IDENT => {
            let name = p.start();
            p.bump();
            name.complete(p, NAME_REF);
            m.complete(p, VAR)
        }
        L_PAREN => {
            p.bump();
            expression(p);
            p.expect(R_PAREN);
            m.complete(p, PAREN)
        }
        IF_KW => {
            p.bump();
            expression(p);
            block(p);
            if p.eat(ELSE_KW) {
                block(p);
            }
            m.complete(p, IF)
        }
        MINUS => {
            p.bump();
            binary(p, PREFIX);
            m.complete(p, NEG)
        }
        _ => {
            m.abandon(p);
            p.expected("an expression");
            return None;
        }
    };
    Some(done)
}

fn arguments(p: &mut Parser<'_>) {
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
        // A missing comma: go on only if another argument starts here.
        if !EXPRESSION_START.contains(&p.current()) {
            break;
        }
    }
    p.expect(R_PAREN);
}
