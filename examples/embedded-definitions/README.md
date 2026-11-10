# Embedded definitions

This example shows a small language project using a definition published by a
separate Rust crate. `geometry-language-extension-example` exports static
`.langmeld` source; the host registers those sources with
`OverlayDefinitionResolver`, which combines them with local project files.
Its in-memory source map is also suitable for fully virtual projects and
editor-owned snapshots. Langmeld does not provide automatic package discovery
or dependency resolution.

The root imports a local `constructs/program.langmeld` file and
`extensions/geometry/drawing.langmeld` from the extension crate. The host adds
the crate source under its project-relative key; the overlay resolver prefers
that source for matching imports and reads other modules from disk.

Run from the workspace root:

```sh
cargo run -p langmeld-embedded-definitions-example --locked
cargo test -p langmeld-embedded-definitions-example --locked
```

The sample output is:

```text
Geometry: draw(circle)
```
