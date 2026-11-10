# Toy language

This example shows a small declarative language split across importable `.lm`
files and parsed with Langmeld's generic parser. It demonstrates grammar
composition and captured syntax fields; it does not define symbol resolution,
types, or runtime behavior.

`language/toy.lm` declares the language and its `program` block. It imports
`language/constructs/statements.lm`, which in turn imports
`language/constructs/values.lm`. The root block lists which statement
constructs it accepts.

Run it from the workspace root:

```sh
cargo run -p langmeld-toy-language-example --locked
cargo test -p langmeld-toy-language-example --locked
```

The program loads the definition and its imports from disk, validates them,
then parses a short WidgetScript source. Its output is:

```text
Language: WidgetScript
Root construct: program
  let: name=title, value="Hello"
  draw: shape=title
```
