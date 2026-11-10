# Embedded definitions

This example shows a small language project using a definition published by a
separate Rust crate. `geometry-language-extension-example` exports static
`.langmeld` source; the host registers those sources with
`InMemoryDefinitionResolver`, which is also suitable for virtual projects and
editor-owned source snapshots. Langmeld does not provide automatic package
discovery or dependency resolution.

The root imports `extensions/geometry/drawing.langmeld`. The extension crate
exports a project-relative source key and text; the host adds both the root
and extension source to the resolver. Langmeld resolves imports relative to
the importing file and composes the declarations before parsing the sample.

Run from the workspace root:

```sh
cargo run -p langmeld-embedded-definitions-example --locked
cargo test -p langmeld-embedded-definitions-example --locked
```

The sample output is:

```text
Geometry: draw(circle)
```
