# Toy language

This working language combines importable `.langmeld` definitions with Rust
construct handlers. It exercises syntax variants (`print` and `say`), optional
fields (`draw ... labeled ...`), repeated string captures (`emit`), finite
choices, nested blocks, line and block comments, and delimiter-bounded
expressions. The `emit` handler reads repeated captures through
`ConstructContext::fields` and `captured_text`. The host evaluates expressions
with its own operator table and semantics; Langmeld supplies the expression
parser but does not define the meaning of operators.

`language/toy.langmeld` declares the language and its `program` block. It imports
`language/constructs/statements.langmeld`, which in turn imports
`language/constructs/values.langmeld`. Both imported constructs implement the
`statement` category, which the root block accepts as a group.

Run it from the workspace root:

```sh
cargo run -p langmeld-toy-language-example --locked
cargo test -p langmeld-toy-language-example --locked
```

The program loads the definition and its imports from disk, validates them,
then executes a short WidgetScript source through registered handlers. Its
sample output is:

```text
Hello, Langmeld!
Drawing circle labeled "main target".
first
second
32
```
