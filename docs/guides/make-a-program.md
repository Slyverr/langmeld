# Make a program

Turn a grammar into a language people run: typed syntax trees, what programs mean, and a
command line.

## Start

```sh
langmeld init shapes-lang
cd shapes-lang
cargo run -- run examples/hello.shapes
```

The new project has a small language that runs at once. Its `README.md` says which file to
change for what, and what to run to check, test and explain it; `cargo run -- test` runs
the cases in `corpus/`. The rest of this page explains the parts. The
[Calc example](../../examples/calc) is a complete language built this way: a grammar, an
interpreter and a command line.

## Typed trees

The build script calls `langmeld::build::generate()`, and the library includes what it
writes:

```rust,ignore
// build.rs
fn main() -> Result<(), langmeld::build::BuildError> {
    langmeld::build::generate()
}

// src/lib.rs
pub mod syntax {
    langmeld::include_syntax!();
}
```

Every node of the grammar becomes a type with a method for each label, and every choice
between rules becomes an enum. Bring `langmeld::ast::AstNode` into scope to use the methods
that every view has, such as `syntax()` and `range()`:

```rust,ignore
use langmeld::ast::AstNode;
use crate::syntax::Statement;

for statement in file.statements() {
    match statement {
        Statement::Let(binding) => { /* binding.name(), binding.value() */ }
        Statement::Print(print) => { /* print.value() */ }
    }
}
```

A part that the source lacks is `None`, so code that handles broken files does not panic.
Adding a rule to a choice makes every `match` on it incomplete, and the compiler points at
each one. To handle every node, implement the generated `Visitor` trait, which has a method
for each node and none with a default. The [generated code reference](../reference/generated-code.md)
lists every name and rule, and `langmeld check --exhaustive` lists the nodes your code
never names.

A label gets no method when a tree cannot tell its part from another of the same kind, such
as an optional `x` followed by a `y` of the same kind. The build warns, and a literal
between them or a rule of its own solves it.

## What programs mean

Langmeld decides nothing about meaning. With the `kit` feature, `langmeld::kit::interp`
offers what tree-walking interpreters need: `Env` for names in nested scopes, `Value` for
common values, and `Budget` so that no program runs for ever or overflows the stack. A
budget that runs out is a diagnostic like any other mistake:

```rust,ignore
self.budget.step().map_err(|reached| {
    Box::new(reached.diagnostic("SHAPES007", self.file, node.range()))
})?;
```

Report the mistakes of running programs as diagnostics, each with a code that the language
explains:

```rust,ignore
Diagnostic::error("SHAPES001", file, node.range(), "`x` is not defined")
    .note("define it with `let`")
```

Use one diagnostic for each mistake, with the whole text of what is wrong and how to solve
it, and test the full message.

## The command line

With the `cli` feature, `langmeld::cli::App` gives the program `check`, `test` and
`explain`, and your own commands described with `clap`:

```rust,ignore
fn main() -> std::process::ExitCode {
    App::new(syntax::loaded())
        .name("shapes")
        .standard()
        .check_with(|file, parsed| vec![/* your own checks */])
        .explanation("SHAPES001", "A name is used that nothing defines.")
        .command(Command::new("render"), |matches| render(matches))
        .run()
}
```

| Command | Does |
|---------|------|
| `check [PATH]... [--format text\|json]` | Parses the files with the language's extensions, runs your checks, and shows every problem |
| `test [--corpus DIR] [--update]` | Runs the corpus files in `corpus/` |
| `explain CODE` | Explains a code of Langmeld or of yours |
| `lsp` | With the `lsp` feature: serves the language to editors, with your checks and the `editor` block of the grammar (`App::new(syntax::loaded())` brings it with the language); see [Editor support](editor-support.md) |

A command ends with exit code 0 when all is well, 1 when it found problems and showed
them, and 2 when it could not do its work. `--format json` writes one JSON object with the
count of files, errors and warnings and every diagnostic with its code, message, place,
labels, notes and fixes, which editors and CI scripts can read.

## Programs of one language

`langmeld init --bins` chooses how the commands are spread over programs, and
`langmeld.toml` records it:

| Layout | Programs | Good for |
|--------|----------|----------|
| `single` (the default) | `shapes` with every command | The simplest install |
| `split` | `shapes` to run programs, `shapesc` for `check`, `test` and `explain` | Small programs, separate releases |
| `multicall` | `shapes`, which acts as `shapes-check` when started under that name | One build with several names |

Every layout is made of the same functions in `src/cli.rs`, so changing it later is a small
edit. The `cli` and `kit` features of `langmeld` are optional, so a crate that only needs
the trees, such as a runtime that must not depend on the parser, leaves them out.

## In a project that exists

`langmeld init --lib crates/shapes-lang` adds one crate to the workspace around that folder,
and adds it to the list of members. It holds what is about the language, from text to
meaning: the grammar, `build.rs`, the typed trees, checks and the interpreter. Name it
after the language, not after Langmeld. It writes no program and changes nothing else, so
programs, runtimes and other crates stay where they are:

```text
shapes-lang/      text to checked tree to meaning (made by `langmeld init --lib`)
shapes-ir/        plain data that the rest shares, with no parser in it
shapes-runtime/   what runs the result; depends on the data only
shapes-cli/       a thin program: `fn main() { shapes_lang::cli::app().run() }`
```

Keep the runtime free of the language crate. The data between them is the boundary, which
gives smaller builds for programs that only run results and a runtime that can be tested
without syntax.
