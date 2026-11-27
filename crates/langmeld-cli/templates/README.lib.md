# {{Name}}

The {{Name}} language, built with [Langmeld](https://github.com/Slyverr/langmeld). This crate
holds everything that is about the language: its grammar, the types of its trees, what
programs mean and the commands of its program.

## Use it

A program is a few lines in a crate of its own, which depends on this one:

```rust,ignore
fn main() -> std::process::ExitCode {
    {{crate}}::cli::app().run()
}
```

Without a program, the `langmeld` command works on the grammar:

```sh
langmeld check                    # problems of the grammar
langmeld test                     # the cases in corpus
langmeld parse --grammar language/main.langmeld examples/hello.{{extension}}
langmeld explain {{CODE}}001          # what a diagnostic means
```

## Change it

| To change | Edit |
|-----------|------|
| What can be written | `language/main.langmeld` |
| What programs do | `src/interp.rs` |
| The commands | `src/cli.rs` |
| What the parser should give | `corpus/*.txt` |

After changing the grammar, build: the compiler shows what the code does not handle yet.
