# Langmeld

A Rust framework for building languages: DSLs, scripting languages, and the frontends of
bigger languages.

## Overview

Every language needs the same hard frontend work: parsing, errors, names and editor support.
Langmeld's goal is that you describe the syntax, structure and name binding once and get
that frontend, while you write what the language means and does in Rust. So far the
language file, Rust types for its trees, diagnostics, a command line, and editor support
from the grammar alone (errors while typing, colours, outline, folding and keyword
completion, in Visual Studio Code and in the browser) exist; names and projects come next.

## Installation

The crates are on crates.io (`langmeld` is the one to depend on). The `langmeld` command can
also be run with [Nix](https://nixos.org/) flakes enabled:

```sh
nix run github:Slyverr/langmeld
nix profile install github:Slyverr/langmeld
```

Prebuilt binaries for Linux, macOS and Windows are on the
[GitHub Releases](https://github.com/Slyverr/langmeld/releases) page. To build from source,
see [Setup](CONTRIBUTING.md#setup).

## Usage

Describe a language in a `.langmeld` file, check it, and parse files with it:

```sh
langmeld check calc.langmeld --explain
langmeld parse --grammar calc.langmeld example.calc
langmeld test .
```

Start a language that runs, with its trees, interpreter and command line already in place:

```sh
langmeld init shapes-lang
cd shapes-lang
cargo run -- run examples/hello.shapes
```

Make a Visual Studio Code extension for it, which starts the language server:

```sh
langmeld package vscode
```

See [Write a `.langmeld` file](docs/guides/write-a-langmeld-file.md) for a first language,
[Make a program](docs/guides/make-a-program.md) to run it,
[Editor support](docs/guides/editor-support.md) for editors, and the
[format reference](docs/reference/langmeld-format.md) for everything a file can say.

## Documentation

In-depth docs (architecture, guides, reference) are in [docs/](docs/README.md).

## Project status

Experimental: the API may change in any minor release. See [CHANGELOG.md](CHANGELOG.md)
for what changed in each version.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, workflow and
conventions.

## License

Licensed under either the [MIT license](LICENSE-MIT) or the
[Apache License 2.0](LICENSE-APACHE), at your option.
