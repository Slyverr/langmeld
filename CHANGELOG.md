# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project uses [Semantic Versioning](https://semver.org/). How to add entries and cut
releases is described in [CONTRIBUTING.md](CONTRIBUTING.md#changelog).

## [Unreleased]

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
