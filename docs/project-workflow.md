# Project workflow

## Create a project

Run the initializer from the Langmeld source checkout:

```sh
cargo run -p langmeld-cli -- init my-language
```

In a terminal it prompts for a Cargo package name and a language display name.
The directory name and package name provide defaults. To skip prompts:

```sh
cargo run -p langmeld-cli -- init my-language \
  --name my-language \
  --language-name MyLanguage \
  --non-interactive
```

The command creates a Cargo project with this layout:

```text
my-language/
├── Cargo.toml
├── README.md
├── src/
│   └── main.rs
└── language/
    ├── language.lm
    └── constructs/
        └── statements.lm
```

The starter `.lm` files describe a small `program` block with `let` and `print`
statements. The Rust host loads the modular definition, parses a sample program,
and prints the result. Language-specific semantics and execution remain Rust
code that the language author adds.

The generated Cargo package is a standalone workspace. The initializer writes
into an existing directory but refuses to overwrite any generated file. It
does not yet generate a project manifest, editor configuration, or build
artifacts.

## Validate the project

From the generated project directory:

```sh
cargo run
```

For `.lm` syntax and import behavior, see [Language definitions](language-definitions.md).
