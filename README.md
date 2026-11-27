# Langmeld

A Rust framework for building languages: DSLs, scripting languages, and the frontends of
bigger languages.

## Overview

Every language needs the same hard frontend work: parsing, errors, names and editor support.
Langmeld's goal is that you describe the syntax, structure and name binding once and get
that frontend, while you write what the language means and does in Rust. Only the language
file and its command line exist so far.

## Installation

Langmeld isn't published on crates.io yet. With [Nix](https://nixos.org/) flakes enabled:

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

See [Write a `.langmeld` file](docs/guides/write-a-langmeld-file.md) for a first language,
and the [format reference](docs/reference/langmeld-format.md) for everything a file can say.

## Documentation

In-depth docs (architecture, guides, reference) are in [docs/](docs/README.md).

## Project status

Experimental. See [CHANGELOG.md](CHANGELOG.md)
for what changed in each version.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, workflow and
conventions.

## License

Licensed under either the [MIT license](LICENSE-MIT) or the
[Apache License 2.0](LICENSE-APACHE), at your option.
