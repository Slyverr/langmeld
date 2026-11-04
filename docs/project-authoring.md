# Project setup and authoring

## Initialize a project

Run the interactive initializer from a Langmeld checkout:

```sh
cargo run -p langmeld-cli -- init
```

Choose whether the project uses declarative `.langmeld` definitions, Rust-authored
language definitions, or both. The initializer can also include a language
CLI. For a non-interactive project scaffold:

```sh
cargo run -p langmeld-cli -- init \
  --path ./my-language \
  --name MyLanguage \
  --no-interactive
```

The chosen options determine the generated files. A declarative project
typically contains:

```text
my-language/
├── langmeld.json
├── language/
│   ├── language.langmeld
│   ├── constructs/
│   ├── expressions/  # optional
│   └── extensions/
├── src/
│   └── host/
└── .langmeld/
    └── generated/
```

`language.langmeld` is the language entry point; construct and extension files keep
definitions composable. Rust host modules live under `src/host/`. Files under
`.langmeld/generated/` are derived outputs. The generated project's README
describes the selected layout and its commands.

New declarative projects use language-model schema 2. The block opener is
inferred from the block declaration, so the root syntax does not repeat it:

```langmeld
language Example {
  version: "0.1.0"
  schema_version: 2
  root: program
}

construct program {
  syntax { default: "program {name}" }
  fields { name: identifier required }
  block: [statement]
}
```

Schema 2 defaults to `{` and `}` with optional newline or semicolon separators.
Configured blocks can declare `open`, `close`, `separator`, and
`separator_policy` entries when a language needs different layout.

Expression grammars and fields require schema 3. They can be declared in the
root file or split into the conventional `language/expressions/` directory:

```langmeld
language Example {
  version: "0.1.0"
  schema_version: 3
  root: program

  expression arithmetic {
    atom: number
    atom: identifier
    operator "-" prefix 30
    operator "+" infix 10 left
    operator "*" infix 20 left
  }

  construct program {
    syntax { root: "program" }
    block: [evaluate]
  }

  construct evaluate {
    syntax { default: "evaluate {value};" }
    fields { value: expression<arithmetic> until ";" required }
  }
}
```

The expression field stores a structured expression tree and consumes through
its declared top-level punctuation terminator. Nested parentheses and brackets
do not end the expression. Expression parsing supplies syntax only; a Rust host
still defines name resolution, types, evaluation, and lowering where needed.
For language-specific atom syntax, add `host_atom_parser: callback_id` to the
expression declaration and implement the generated callback in `src/host/parser.rs`.
The optional Tree-sitter exporter currently declines grammars with expression
fields instead of emitting a partial grammar.

Schema 4 adds an optional lexical policy for comments. If omitted, the built-in
lexer retains its legacy `#` line comments. An authored `lexical` section
replaces that policy; it can set one line-comment delimiter and one non-nested
block-comment pair. Omitting `line_comment` inside the section disables line
comments:

```langmeld
language Example {
  version: "0.1.0"
  schema_version: 4
  root: program

  lexical {
    line_comment: "//"
    block_comment: "/*" "*/"
  }
}
```

The built-in source and expression lexers, TextMate grammar, and editor comment
configuration share these delimiters. The Tree-sitter exporter supports a
declared line-comment delimiter; it reports configured block comments as
unavailable until it can represent them exactly. Languages needing nested or
stateful comments can provide a custom Rust lexer.

Schema 5 lets a language set generic name-resolution behavior once per symbol
kind in its root definition. Without an entry, declarations are hoisted,
duplicates are errors that keep the first declaration as the navigation
target, and nested scopes may shadow outer names. Policies can make declarations
sequential within each file, permit duplicates and diagnose references that
become ambiguous, or reject shadowing. Declarations in other files remain
visible regardless of source-list order; imports and module visibility are not
inferred from file order.

```langmeld
language Example {
  version: "0.1.0"
  schema_version: 5
  root: program

  symbol_policy variable: sequential allow_ambiguous disallow
}
```

An ambiguous reference has a diagnostic and no navigation target. Languages
with overload resolution or other custom name rules can replace the generic
workspace resolver with a Rust host callback.

## Develop the language

Edit the project sources, then regenerate and check derived output:

```sh
langmeld generate --project-root ./my-language
langmeld generate --project-root ./my-language --check
```

`--check` reports stale generated files without rewriting them. In generated
declarative projects, `langmeld generate --install-host` refreshes generated
host modules while preserving marked callback bodies. Rust-authored and
combined projects use their compiled host CLI instead:

```sh
cargo run -- generate
cargo run -- generate --check
```

The host CLI loads the compiled effective language model, including local
extensions. It rebuilds only `.langmeld/generated/`; it does not edit
`src/host/`. Library-only projects can call
`generate_tooling`, `generate_host_scaffold`, and
`write_generated_artifacts` through `langmeld::language`:

```rust
let mut artifacts = langmeld::language::generate_tooling(&language)?;
artifacts.extend(langmeld::language::generate_host_scaffold(&language));
langmeld::language::write_generated_artifacts(project_root, &artifacts)?;
```

Review generated changes along with the language definition changes that
produced them.

Use `langmeld configure` to interactively adjust program source roots,
individual files, or source extensions. The command validates the candidate
language and source selection before updating `langmeld.json`.

`langmeld check --project langmeld.json` checks the configured sources and
generic workspace analysis. If a language requires callbacks implemented in
Rust, use its generated host or another integration that registers those
callbacks; the standalone check command does not load custom host code.

## Extensions

An extension adds declarative constructs to a language. Project manifests can
refer to `.langmeld` files directly or to a local versioned package. Project sources
and packages declare their required language version; `langmeld lock` records
the resolved files and package data in `langmeld.lock`.

For example, a local package can be added with:

```sh
langmeld extension add \
  --id community.ui \
  --version 1.0.0 \
  --path ../ui-extension/language/ui.langmeld \
  --language-version '^0.1.0'
```

Resolution is local and lockfile-based; Langmeld does not download packages.
Declarative extensions add language definitions, not executable Rust code.
Custom parsing, analysis, runtime, and backend behavior belongs in the
language host.

For an end-to-end modular definition and extension, see
[`declarative-interpreter`](../examples/declarative-interpreter/README.md).
