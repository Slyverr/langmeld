# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project uses [Semantic Versioning](https://semver.org/). How to add entries and cut
releases is described in [CONTRIBUTING.md](CONTRIBUTING.md#changelog).

## [Unreleased]

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
