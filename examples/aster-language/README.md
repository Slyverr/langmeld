# Aster

Aster is a small numeric language project that demonstrates how a Rust host can
combine modular `.langmeld` definitions with custom analysis and execution.
Its syntax defines functions with parameter scopes, numeric expressions, calls,
and output. An independently composed assertion extension adds syntax; the
Rust host supplies its runtime handler in `src/extensions/`.

Langmeld parses source and resolves declared symbols. Aster adds numeric-name
and function-arity checks, lowers the typed AST into a host-owned IR, runs an
ordered constant-folding pass, and interprets the result. Numeric semantics,
the IR, optimization, and runtime behavior remain Aster code.

The project was initialized with the Langmeld CLI. Definitions are split among
`language/language.langmeld`, `language/constructs/`, and
`language/extensions/`. Generated editor and host tooling lives under
`.langmeld/generated/`; rebuild it with `cargo run --bin aster-host -- generate`.

## Try it

From this directory:

```sh
cargo test --locked
cargo run --bin aster-host -- check --project
cargo run --bin aster-host -- run --project
```

The example prints `value = 21` and `answer = 42`. Change the assertion in
`examples/hello.aster.lmprog` to see the extension's host handler report a
runtime failure.
