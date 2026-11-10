# Toy language

This example shows a small declarative language split across importable
`.langmeld` files and parsed with Langmeld's generic parser. It demonstrates
grammar composition, primitive and finite-choice captures, and
delimiter-bounded expression capture. Its construct declarations also mark
keyword completions explicitly. The host passes the captured source span to
`langmeld-expression` with its own operator table; Langmeld does not infer
expression meaning, types, or runtime behavior.

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
then parses a short WidgetScript source. Its output is:

```text
Language: WidgetScript
Root construct: program
  let: name=title, value="Hello"
  draw: shape=circle
  compute: value=20 + 4 * (2 + 1)
```
