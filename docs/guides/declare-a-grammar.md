# Declare a grammar

Describe a language with the builder and Langmeld parses it: no parsing code, a complete
syntax tree whatever the source says, and one diagnostic per mistake.

## A whole program

This reads settings such as `retries = 2 + 1`, one per line.

```rust
use langmeld_syntax::pattern::{field, lit, many, r, terminated};
use langmeld_syntax::{
    Assoc, ExprSpec, FileId, IdentStyle, IntPreset, Language, Preset, SourceFile, TextTooLarge,
    Token, parse, seq,
};

fn main() -> Result<(), TextTooLarge> {
    let language = Language::builder("Settings")
        // Tokens rules refer to by name. Literals such as `=` need no declaration.
        .token(Token::skip("space", r"[ \t\r\n]+"))
        .token(Token::preset("name", Preset::Ident(IdentStyle::Xid)))
        .token(Token::preset("number", Preset::Int(IntPreset::default())))
        .line_comment("#")
        // The first rule is where a file starts.
        .rule("File", many(terminated(r("Setting"))))
        .rule(
            "Setting",
            seq![field("key", r("name")), lit("="), field("value", r("Expr"))],
        )
        .expr(
            ExprSpec::new("Expr")
                .atom("Number", r("number"))
                .atom("Group", seq![lit("("), r("Expr"), lit(")")])
                .infix("Product", ["*"], 2, Assoc::Left)
                .infix("Sum", ["+", "-"], 1, Assoc::Left),
        )
        // How errors name a rule.
        .describe("Setting", "a setting")
        .build()
        .expect("the grammar is valid");

    let source = SourceFile::new(
        FileId::new(0),
        "settings",
        "retries = 2 + 1\nwait = (3 *\ntimeout = 9\n",
    )?;
    let parsed = parse(&language, &source);

    assert_eq!(parsed.syntax().text(), source.text());
    let settings = parsed.syntax().children().count();
    assert_eq!(settings, 3);
    assert_eq!(parsed.diagnostics[0].code, "LM-P001");
    Ok(())
}
```

## Rules

| Write | Meaning |
|-------|---------|
| `lit("let")` | Exactly this text. Words are reserved unless declared contextual |
| `r("Name")` | A rule or a token |
| `seq![a, b]` | One after another |
| `choice![a, b]` | One of them, chosen by the next tokens |
| `opt(a)`, `many(a)`, `many1(a)` | Optional, repeated, repeated at least once |
| `sep(a, ",")`, `sep_strict(a, ",")` | A list, with or without a trailing separator |
| `terminated(a)` | Followed by the end of the line or a terminator |
| `field("name", a)` | A labelled part, listed in the node schema |
| `look(a)`, `not(a)` | What comes next, without taking it |
| `attempt(a)` | Parse it if that works without errors |

Every rule becomes a node of its name, except a rule that only chooses between other rules.
Operators go in an [`ExprSpec`](../../crates/langmeld-syntax/src/grammar/builder.rs), where
a larger precedence binds tighter.

## Refused grammars

`build` checks the whole grammar and returns every problem with a stable
[code](../reference/diagnostics.md) and advice. A grammar is refused rather than read one
way silently: for example when two choices cannot be told apart within three tokens, or a
rule can start with itself.

## When a grammar is not enough

An [`ExternRule`](../../crates/langmeld-syntax/src/grammar/builder.rs) parses one rule with
Rust code on the [parser primitives](hand-written-parser.md), and can call declared rules
back with `Parser::rule`. Say what it can start with (`first`), which literals it reads
(`uses`) and which nodes it makes (`produces`). A custom rule without `first` may start with
anything, which is reported as a warning (`LM-G015`) because the grammar cannot check the
choices around it.

## Next

The [Calc](../../examples/calc/src/lib.rs) and [JSON](../../examples/json/src/lib.rs)
examples.
