# Langmeld

Langmeld is an early-stage Rust language framework. The workspace currently
contains `langmeld-core`, which provides source snapshots, stable source
identities, byte spans, structured diagnostics, and the serializable schema for
language definitions. The CLI and language processing crates have not been
added yet.

## Workspace checks

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

## Project guidance

- [CONTRIBUTING.md](CONTRIBUTING.md) describes the staged workflow, verification,
  and commit conventions.
- [AGENTS.md](AGENTS.md) gives coding agents the repository-specific rules and
  links to the project-local Rust skills under `.agents/skills/`.


## License

Licensed under either of these terms, at your option:

- [MIT](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)
