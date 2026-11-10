# Embedded definitions

This example shows how a Rust host can compose a project `.langmeld` file with
a definition module embedded in a Rust crate. It demonstrates the
`DefinitionResolver` extension point; it does not provide automatic package
discovery or dependency resolution.

The root imports `extensions/drawing.langmeld`. `BundledDefinitions` maps that
import to source included in the binary with `include_str!`, assigns it a
stable source key, and lets Langmeld parse and validate both sources together.
A third-party crate can expose its own `.langmeld` text the same way; the
language host decides which modules are available and how imports map to them.

Run from the workspace root:

```sh
cargo run -p langmeld-embedded-definitions-example --locked
cargo test -p langmeld-embedded-definitions-example --locked
```

The sample output is:

```text
Geometry: draw(circle)
```
