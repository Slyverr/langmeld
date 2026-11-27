# {{Name}}

A language built with [Langmeld](https://github.com/Slyverr/langmeld).

## Run it

You need [Rust](https://rustup.rs/).

```sh
{{run}} run examples/hello.{{extension}}     # prints 42
{{tools}} check examples                   # problems of every file there
{{tools}} test                             # the cases in corpus
{{tools}} explain {{CODE}}001                  # what a diagnostic means
```

## Change it

| To change | Edit |
|-----------|------|
| What can be written | `language/main.langmeld` |
| What programs do | `src/interp.rs` |
| The commands | `src/cli.rs` |
| What the parser should give | `corpus/*.txt` |

After changing the grammar, build: the compiler shows what the code does not handle yet.
`{{tools}} test --update` writes what the parser makes of new cases into the corpus;
read the result before keeping it.

The [`langmeld` command](https://github.com/Slyverr/langmeld/blob/main/docs/reference/cli.md)
checks the grammar and tries files without building: `langmeld check`, `langmeld parse
examples/hello.{{extension}}`.
