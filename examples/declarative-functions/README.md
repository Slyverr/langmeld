# Declarative Functions

This example uses `.langmeld` for a small function grammar and ordinary Rust
for the language-specific AST and semantic rules. It checks parameterized
function declarations, forward calls, unresolved names, and call arity. It does
not define a runtime or compiler.

The grammar lives in `language/`. Rust lowers generic parsed constructs into a
typed AST in `src/ast.rs`; `src/analysis.rs` collects function signatures and
then resolves calls in two ordered `LanguagePass` stages. This demonstrates
that syntax declarations do not implicitly create symbols or determine
function semantics.

Run the valid sample or analyze a source file:

```sh
cargo run -p langmeld-declarative-functions-example --locked
cargo run -p langmeld-declarative-functions-example --locked -- program.functionflow
```

The core syntax includes parameter lists and return types:

Parameters are comma-separated; missing and trailing commas are syntax errors.

```functionflow
program {
    fn main() -> i32 {
        invoke add(20, 22);
    }

    fn add(left: i32, right: i32) -> i32 {}
}
```

`main` calls `add` before its declaration. The first analysis pass collects
all signatures, so the second pass can resolve that forward call.
