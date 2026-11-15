# Declarative Functions

This example uses `.langmeld` for a small function grammar and ordinary Rust
for the language-specific AST and semantic rules. It checks parameterized
function declarations, forward calls, unresolved names, and call arity. It does
not define a runtime or compiler.

The grammar lives in `language/`. Its build script generates typed capture
views and embeds the root definition and its imports under Cargo's `OUT_DIR`;
`src/ast.rs` uses the views to lower parsed constructs into a language-specific
AST. The example loads its grammar from those embedded sources, so runtime
language loading does not need project filesystem access. `src/analysis.rs`
collects function signatures and then resolves calls in two ordered
`LanguagePass` stages. Syntax declarations do not implicitly create symbols or
determine function semantics. Named syntax groups generate optional and
repeated Rust views, keeping each parameter's name and type together.

The generated bindings and source list are included from the stable module in
`src/lib.rs`. These generated files are build output and should not be checked
in; Cargo rebuilds them when the manifest or imported definitions change.

The project-local language server is in the same package. Run it with:

```sh
cargo run -p langmeld-declarative-functions-example --bin functionflow-lsp --locked
```

`src/lsp.rs` adapts the declarative parser, typed AST lowering, and ordered
analysis passes to `HostLanguageBackend`. The server publishes both syntax and
host semantic diagnostics, with grammar-based completion. It does not add a
separate language-server package or infer the function rules from `.langmeld`.

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
