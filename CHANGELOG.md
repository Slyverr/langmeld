# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project uses [Semantic Versioning](https://semver.org/). How to add entries and cut
releases is described in [CONTRIBUTING.md](CONTRIBUTING.md#changelog).

## [Unreleased]

### Added

- The `langmeld` command: `check` validates a language file or project and, with `--explain`,
  shows where it looks more than one token ahead; `test` runs the corpus files of a
  language project and `--update` rewrites what they expect; `parse --grammar` parses a
  file with a `.langmeld` file alone, reading custom rules as one error each.
- A language can be compiled ahead of time from a build script (`build::embed`) and a program
  includes the result, so it starts without reading or checking the grammar. A language
  declared with the builder can be written as bytes and read back too.
- A language can be written in a `.langmeld` file and loaded with `load_language`. The
  file is checked, and every mistake is one diagnostic at its place in the file, with a
  note that says how to fix it. Files can import others, and custom rules and tokens get
  their Rust code by name.
- `langmeld.toml`, the file of a language project, is read with every mistake reported at
  its place: the root file, how binaries are laid out, the project file and the editor
  package. It cannot repeat what the `language` block of the grammar already says.
- Strings with holes can be declared in a grammar (`Token::interpolated`): the quotes, the
  text and each hole's expression become one node.
- Grammar problems can point at the place a rule or name was written
  (`LanguageBuilder::locate`, `Pattern::located`), and `Language::lookahead` lists the
  choices that need more than one token to decide.

## [0.3.0] - 2026-11-27

### Added

- Declare a language as a grammar with a builder: rules, repetition, choices, operators
  with precedence, fields, and custom rules written in Rust. The grammar is checked when
  it is built, and each problem comes with advice on how to fix it.
- A parser that runs a declared grammar and recovers from mistakes, giving a lossless
  syntax tree and one diagnostic per mistake.
- Hand-written parsers can call rules by name, try a part and go back, and look ahead.
- A description of the nodes a grammar produces and the fields each one has.
- An example that declares JSON as a grammar, and another that declares Calc and is checked
  to give the same trees as its hand-written twin.

### Fixed

- A parser that had to leave out the root of a very deep tree could give a tree one level
  taller than its depth limit.

## [0.2.0] - 2026-11-27

### Added

- A lexer built from declared tokens: patterns and presets for numbers, strings, characters
  and identifiers, reserved and contextual keywords, nested comments, newline-ended
  statements, string holes, and tokens lexed by your own code.
- Building blocks for a hand-written parser that give a lossless syntax tree and one
  diagnostic per mistake, with limits that report instead of crashing.

## [0.1.0] - 2026-11-27

### Added

- Core types for languages: source files with identities, byte ranges, line and column
  positions in UTF-8 and UTF-16, diagnostics with labels, notes and fixes, and syntax
  kinds for `rowan` trees.
