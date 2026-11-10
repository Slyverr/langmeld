# Changelog

All notable changes to Langmeld are documented here.

The format follows [Keep a Changelog 2.0.0](https://keepachangelog.com/en/2.0.0/),
and this project uses [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-10

### Added

- Source snapshots, stable source identities, revisions, byte spans, and
  structured diagnostics in `langmeld-core`.
- A validated grammar model and recovering generic parser for token patterns,
  primitive captures, syntax alternatives, optional and repeated groups, and
  explicitly scoped nested blocks in `langmeld-language`.
- A first `.lm` definition frontend parsed through the generic engine, plus a
  root-level Toy language consumer example.
- A host lexer contract that validates token text and source ranges before the
  generic parser consumes them.
