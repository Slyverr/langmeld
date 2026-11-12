# Aster

Aster is a small Rust-first language consumer that combines Langmeld's source,
diagnostic, host-parser, host-analyzer, and expression APIs. It deliberately
uses its own AST and implements its own name resolution and basic type checking.
It uses `LanguageParser` instead of `.langmeld` because its function grammar and
expression boundaries need parser-directed handling.

The example supports function declarations, `i32`/`bool`/`string` values,
`let` bindings, returns, nested lexical blocks, function calls, and common
arithmetic/comparison expressions. Its analyzer demonstrates forward function
references, local declaration order, block scopes, shadowing, duplicate names,
call arity, and type diagnostics. `AsterNavigator` maps definition and
references requests through the analyzer's resolved symbol IDs, so navigation
respects shadowing instead of matching names textually. Its rename hook checks
the proposed edit by reanalyzing the result, rejecting same-scope collisions
and changes that would redirect a resolved reference. Aster has no runtime or
compiler.

Function declarations include ordinary parameter lists and return types:

```aster
fn add(left: i32, right: i32) -> i32 {
    let total = left + right;
    return total;
}
```

This example uses a Rust-authored parser because its grammar and expression
boundaries need parser-directed handling. For the declarative `.langmeld`
route, start with [`toy-language`](../toy-language/README.md).

The analyzer uses two ordered `LanguagePass` implementations: it collects
function signatures first, then analyzes function bodies. This permits forward
function references while keeping Aster's symbol and type policies in the
example rather than in Langmeld.

Run it with:

```sh
cargo run -p langmeld-aster-language-example --locked
```

Start the host-integrated language server with:

```sh
cargo run -p langmeld-aster-language-example --bin aster-lsp --locked
```

The server publishes Aster's parser and semantic diagnostics and uses its
resolved symbol table for definition, references, rename, document symbols,
and semantic tokens. The document outline groups parameters and local
declarations under their containing functions. At top-level blank lines, it
offers a host-authored function declaration snippet; it does not offer that
snippet inside a function body.

The semantic rules here belong to this language. Langmeld currently provides
the parser contracts and expression tree; it does not generate symbol tables,
scope rules, type checking, or execution behavior from this example.
