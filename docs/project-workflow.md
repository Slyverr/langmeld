# Project workflow

## Create a project

Run the initializer from the Langmeld source checkout:

```sh
cargo run -p langmeld-cli -- init my-language
```

In a terminal it prompts for a syntax approach, Cargo package name, and language
display name. The directory name and package name provide defaults, and invalid
directory-derived names are normalized to valid Cargo package names. Invalid
prompt input is rejected in place so it can be corrected without restarting.
To skip prompts for the declarative template:

```sh
cargo run -p langmeld-cli -- init my-language \
  --name my-language \
  --language-name MyLanguage \
  --non-interactive
```

Choose the custom parser template when you want Rust to own all syntax and AST
behavior:

```sh
cargo run -p langmeld-cli -- init my-language \
  --template custom-parser \
  --non-interactive
```

The declarative template creates this project layout:

```text
my-language/
├── Cargo.toml
├── .gitignore
├── LICENSE-MIT
├── LICENSE-APACHE
├── langmeld.toml
├── README.md
├── src/
│   ├── lib.rs
│   └── main.rs
├── tests/
│   └── language.rs
└── language/
    ├── language.langmeld
    └── constructs/
        └── statements.langmeld
```

The starter `.langmeld` files describe a small `program` block with `let` and `print`
statements. `src/lib.rs` loads the modular definition and formats diagnostics;
`src/main.rs` provides a runnable CLI that parses a built-in sample or a source
file path. Public behavior tests live in `tests/language.rs`. Language-specific
semantics and execution remain Rust code that the language author adds.

Run the sample or pass a source file to the generated CLI:

```sh
cargo run
cargo run -- path/to/program.widget
cargo test
```

The custom parser template creates a library in `src/lib.rs`, a demonstration
binary in `src/main.rs`, a stdio LSP binary in `src/bin/language-lsp.rs`, and
an integration test in `tests/parser.rs`. Its `SayParser` implements
`LanguageParser` directly, demonstrating a host-owned syntax and parse tree
without `.langmeld` files or a Langmeld manifest. The LSP binary publishes parser
diagnostics through `HostLanguageBackend`; add host analysis, completion, or
navigation providers there as needed. Both generated CLIs parse a built-in
sample with `cargo run` or a source file with `cargo run -- path/to/source`.
Use `cargo test` to run the generated integration tests.

`langmeld.toml` points to the root `.langmeld` file and has a format version:

```toml
format = 1
definition = "language/language.langmeld"
```

Format `1` accepts only these two fields. The definition path must be a
project-relative `.langmeld` path without parent-directory segments.

The generated Cargo package is a standalone workspace. The initializer writes
into an existing directory but refuses to overwrite any generated file. Its
dual-license files match the manifest; replace the copyright placeholders in
`LICENSE-MIT`. The generated README reminds you to fill in the copyright
holder. It does not yet generate editor configuration or build artifacts.

## Validate the project

Validate the project from the Langmeld checkout with:

```sh
cargo run -p langmeld-cli -- check \
  --manifest-path my-language/langmeld.toml
```

After installing the Langmeld CLI, run `langmeld check` from a declarative
project directory. The command validates the manifest and complete imported
definition tree; it does not parse user program files or run language-specific
semantics. Custom parser projects should use Cargo to build and run the host.
Run `langmeld format` from a project directory to format every `.langmeld` file
reachable from its manifest. Use `--manifest-path path/to/langmeld.toml` when
running it elsewhere. Pass a file path to format one standalone definition.
Use `--check` in CI to verify formatting without changing files; project mode
validates the import graph before formatting any file. Set indentation with
`--tab-width 2` or use `--tabs`.
For declarative projects, `langmeld lsp` starts an editor server that publishes
parse diagnostics and grammar-scoped syntax completions. Run
`langmeld lsp --definitions` to get parser diagnostics and contextual syntax
completion while editing `.langmeld` files. It automatically uses `langmeld.toml` in
the current directory when present, or accepts an explicit `--manifest-path`
to validate the imported project graph against all open project file buffers.
In manifest mode, it also suggests composed construct and category names in
definition reference fields, and local `.langmeld` files and folders in import
paths. When supported by the editor, it watches project `.langmeld` files and
refreshes the open definition analysis when one changes on disk. Open buffers
continue to take precedence over disk contents.
See
[Editor integration](editor-integration.md) for supported features and host
extension points.
Generate a lexical TextMate grammar to standard output with:

```sh
langmeld generate textmate \
  --manifest-path langmeld.toml \
  --scope-name source.widget \
  --file-type widget
```

The command resolves imported definitions through the project manifest. The
scope name is required; file types are optional and can be repeated. Redirect
the output to a file or pipe it to another tool. See the editor integration
guide for the grammar's lexical-only limits.
For the declarative example host, run:

```sh
cd my-language
cargo run
```

For `.langmeld` syntax and import behavior, see
[Language definitions](language-definitions.md).
