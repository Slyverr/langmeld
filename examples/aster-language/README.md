# Aster

Aster is a small Rust-first language consumer that combines Langmeld's source,
diagnostic, host-parser, host-analyzer, and expression APIs. It deliberately
uses its own AST and implements its own name resolution and basic type checking.
It uses `LanguageParser` instead of `.lm` because its function grammar and
expression boundaries need parser-directed handling.

The example supports function declarations, `i32`/`bool`/`string` values,
`let` bindings, returns, nested lexical blocks, function calls, and common
arithmetic/comparison expressions. Its analyzer demonstrates forward function
references, local declaration order, block scopes, shadowing, duplicate names,
call arity, and type diagnostics. Aster has no runtime or compiler.

Run it with:

```sh
cargo run -p langmeld-aster-language-example --locked
```

The semantic rules here belong to this language. Langmeld currently provides
the parser contracts and expression tree; it does not generate symbol tables,
scope rules, type checking, or execution behavior from this example.
