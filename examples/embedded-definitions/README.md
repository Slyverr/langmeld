# Embedded definitions

This example shows how a Rust host can compose `.langmeld` sources embedded in
Rust crates. It uses `InMemoryDefinitionResolver`, which is also suitable for
virtual projects and editor-owned source snapshots. It does not provide
automatic package discovery or dependency resolution.

The root imports `extensions/drawing.langmeld`. The host adds both the root and
the included module to the resolver using project-relative keys; imports are
resolved relative to the importing file. A third-party crate can expose its
own `.langmeld` text for the host to register.

Run from the workspace root:

```sh
cargo run -p langmeld-embedded-definitions-example --locked
cargo test -p langmeld-embedded-definitions-example --locked
```

The sample output is:

```text
Geometry: draw(circle)
```
