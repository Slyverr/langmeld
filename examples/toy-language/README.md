# Toy language

This example shows a small working language split across importable
`.langmeld` files and Rust construct handlers. It demonstrates grammar
composition, primitive and finite-choice captures, source-located runtime
errors, and delimiter-bounded expression capture. The host evaluates
expressions with its own operator table and semantics; Langmeld supplies the
expression parser but does not define the meaning of operators.

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
output is:

```text
Language: WidgetScript
Hello, Langmeld!
Drawing circle.
32
```
