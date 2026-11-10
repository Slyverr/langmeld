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
├── langmeld.toml
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

`langmeld.toml` points to the root `.lm` file and has a format version:

```toml
format = 1
definition = "language/language.lm"
```

Format `1` accepts only these two fields. The definition path must be a
project-relative `.lm` path without parent-directory segments.

The generated Cargo package is a standalone workspace. The initializer writes
into an existing directory but refuses to overwrite any generated file. It
does not yet generate a project manifest, editor configuration, or build
artifacts.

## Validate the project

Validate the project from the Langmeld checkout with:

```sh
cargo run -p langmeld-cli -- check \
  --manifest-path my-language/langmeld.toml
```

After installing the Langmeld CLI, run `langmeld check` from the generated
project directory. The command validates the manifest and complete imported
definition tree; it does not parse user program files or run language-specific
semantics. Run the example host from the generated project with:

```sh
cd my-language
cargo run
```

For `.lm` syntax and import behavior, see
[Language definitions](language-definitions.md).
