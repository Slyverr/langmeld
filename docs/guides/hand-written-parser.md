# Write a parser by hand

Langmeld's primitives give a parser of your own a complete syntax tree, positions and good
errors. You declare the tokens and write one function per rule.

## A whole program

This reads lines such as `total = a + 2;`.

```rust
use langmeld_syntax::{
    FileId, IdentStyle, IntPreset, Lexer, Parser, Preset, SourceFile, SyntaxKind, TextTooLarge,
    TokenSpec,
};

// Token kinds.
const SPACE: SyntaxKind = SyntaxKind(0);
const NAME: SyntaxKind = SyntaxKind(1);
const NUMBER: SyntaxKind = SyntaxKind(2);
const EQ: SyntaxKind = SyntaxKind(3);
const PLUS: SyntaxKind = SyntaxKind(4);
const SEMI: SyntaxKind = SyntaxKind(5);
// Node kinds.
const FILE: SyntaxKind = SyntaxKind(6);
const ASSIGN: SyntaxKind = SyntaxKind(7);
const VALUE: SyntaxKind = SyntaxKind(8);
const SUM: SyntaxKind = SyntaxKind(9);

fn file(p: &mut Parser<'_>) {
    let node = p.start();
    while !p.at_end() {
        if p.current() == NAME {
            assignment(p);
        } else {
            // Skip what has no place here, and report it once.
            p.recover("an assignment", &[NAME]);
        }
    }
    node.complete(p, FILE);
}

fn assignment(p: &mut Parser<'_>) {
    let node = p.start();
    p.bump();
    p.expect(EQ);
    sum(p);
    p.expect(SEMI);
    node.complete(p, ASSIGN);
}

fn sum(p: &mut Parser<'_>) {
    let Some(mut left) = value(p) else { return };
    while p.current() == PLUS {
        // What was read turns out to be the left side of a sum.
        let node = left.precede(p);
        p.bump();
        value(p);
        left = node.complete(p, SUM);
    }
}

fn value(p: &mut Parser<'_>) -> Option<langmeld_syntax::CompletedMarker> {
    if !matches!(p.current(), NAME | NUMBER) {
        p.expected("a value");
        return None;
    }
    let node = p.start();
    p.bump();
    Some(node.complete(p, VALUE))
}

fn main() -> Result<(), TextTooLarge> {
    let lexer = Lexer::builder()
        .token(TokenSpec::regex(SPACE, "space", r"\s+").skip())
        .token(TokenSpec::preset(NAME, "name", Preset::Ident(IdentStyle::Xid)))
        .token(TokenSpec::preset(NUMBER, "number", Preset::Int(IntPreset::default())))
        .token(TokenSpec::punct(EQ, "="))
        .token(TokenSpec::punct(PLUS, "+"))
        .token(TokenSpec::punct(SEMI, ";"))
        .build()
        .expect("the declarations are valid");

    let source = SourceFile::new(FileId::new(0), "example", "total = a + 2\nx = ;")?;
    let mut parser = Parser::new(&source, lexer.lex(&source), &lexer);
    file(&mut parser);
    let parse = parser.finish();

    // The tree holds every byte, whatever is wrong with it.
    assert_eq!(parse.syntax().text(), source.text());
    let messages: Vec<_> = parse.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        ["expected `;`, found `x`", "expected a value, found `;`"]
    );
    Ok(())
}
```

## What the primitives do for you

- **Lossless trees.** Whitespace, comments and unknown characters stay in the tree. They
  sit before the node that follows them; documentation comments go inside it.
- **One mistake, one diagnostic.** After an error, further ones are held back until a token
  is taken as expected.
- **Expected tokens.** `at`, `eat` and `expect` remember what they asked for, so
  `error_expected` can say "expected `,` or `)`". `current` and `nth` only look.
- **Limits.** Deep nesting, very tall trees and a parser that stops taking tokens are
  reported instead of overflowing or hanging.

Two things remain yours: every loop must take a token or stop on each turn, and recovery
needs to know where it can resume.

A complete language written this way is in
[`examples/handwritten-calc`](../../examples/handwritten-calc/src/lib.rs).
