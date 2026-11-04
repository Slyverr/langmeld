# Ledger language example

Ledger is a small interpreted language project scaffolded with Langmeld. Its
grammar lives in `language/constructs/program.langmeld`; its Rust host evaluates the
generic syntax tree in `src/lib.rs`.

The example supports numeric variable declarations, addition, expression
evaluation, and printing:

```text
program {
  set first to 19
  set second to 23
  add first and second as total
  subtract first from total as remainder
  calculate computed as (19 + 23) * 2;
  print total
  print remainder
  print computed
}
```

The base definition declares a `statement` category and the `set`, `add`, and
`print` constructs. `language/extensions/subtract.langmeld` adds the subtraction
construct through that category, so the extension does not need to edit the
root block. That file contributes syntax and data only. The Rust host explicitly
dispatches the `subtract` construct from `src/lib.rs` to
`src/extensions/subtract.rs`, where the runtime operation is implemented.
Langmeld does not load executable code from declarative extensions; the host
application must compile and register any corresponding behavior. Langmeld
parses the source and resolves references across the workspace. The host checks
execution order and prints `total = 42`, `remainder = 23`, and `computed = 84`.
The `calculate` syntax
captures a schema-v3 expression; its Rust runtime defines numeric atoms and
operator behavior. Parentheses and precedence come from Langmeld's expression
parser, while evaluation remains owned by this example.

Run the commands from the example directory:

```sh
cd examples/declarative-interpreter
cargo test --offline --locked
cargo run --offline -- run --project
```

The example is a standalone Cargo workspace so it exercises Langmeld as a
consumer package while using the local checkout through a path dependency.

After changing an extension, refresh its definition lock, then regenerate and
verify tooling from the repository root:

```sh
cargo run -p langmeld-cli -- lock --project examples/declarative-interpreter/langmeld.json
cargo run -p langmeld-cli -- generate --project-root examples/declarative-interpreter
cargo run -p langmeld-cli -- generate --project-root examples/declarative-interpreter --check
```

The generated grammar, highlighting metadata, typed AST, and references include
the extension construct.
