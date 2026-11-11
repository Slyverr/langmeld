# Custom parser

This small calculator shows the Rust-owned parser route. The example supplies
its own lexer, parser, AST, and runtime, and uses `langmeld-expression` to parse
operator precedence and parenthesized expressions. It does not use `.langmeld`
or the generic construct-handler registry.

Run it from the workspace root:

```sh
cargo run -p langmeld-custom-parser-example --locked
```

The built-in program evaluates to:

```text
height = 30
width = 28
```

Use this as a minimal comparison to the more complete
[`aster-language`](../aster-language), which adds name resolution, semantic
analysis passes, and an LSP backend.
