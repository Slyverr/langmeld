# Declarative Functions

This example uses `.langmeld` for a small function grammar and ordinary Rust
for the language-specific AST and semantic rules. It checks parameterized
function declarations, forward calls, unresolved names, and call arity. It does
not define a runtime or compiler.

The grammar lives in `language/`. Its build script generates typed capture views
under Cargo's `OUT_DIR`; `src/ast.rs` uses those views to lower parsed constructs
into a language-specific AST. `src/analysis.rs` collects function signatures
and then resolves calls in two ordered `LanguagePass` stages. This demonstrates
that syntax declarations do not implicitly create symbols or determine
function semantics. Named syntax groups generate optional and repeated Rust
views, keeping each parameter's name and type together.

The generated bindings are included from the stable source module in
`src/lib.rs`. The generated file is build output and should not be checked in.

Run the valid sample or analyze a source file:

```sh
cargo run -p langmeld-declarative-functions-example --locked
cargo run -p langmeld-declarative-functions-example --locked -- program.functionflow
```

The core syntax includes parameter lists and return types:

Parameters are comma-separated; missing and trailing commas are syntax errors.
The `invoke` construct accepts both `invoke add(...)` and `call add(...)`;
the example lowers both named syntax variants to the same call representation.

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
