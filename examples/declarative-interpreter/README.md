# Ledger language example

Ledger is a small interpreted language project scaffolded with Langmeld. Its
grammar lives in `language/constructs/program.lm`; its Rust host evaluates the
generic syntax tree in `src/lib.rs`.

The example supports numeric variable declarations, addition, and printing:

```text
program {
  set first to 19
  set second to 23
  add first and second as total
  print total
}
```

The `.lm` definition declares `set` and `add` names as variables and marks
`add` and `print` operands as references. Langmeld parses the source and
resolves those references across the workspace. The host then evaluates the
resulting AST, checks execution order, and prints `total = 42`.

Run the commands from the example directory:

```sh
cd examples/declarative-interpreter
cargo test --offline --locked
cargo run --offline -- run --project
```

The example is a standalone Cargo workspace so it exercises Langmeld as a
consumer package while using the local checkout through a path dependency.
