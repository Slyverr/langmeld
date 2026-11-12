# Typed Rust bindings

The generic parser returns named fields and capture groups through runtime
collections. When a Rust implementation wants compile-time checked access to
those names, the optional `langmeld-build` crate generates typed views from the
project's validated `.langmeld` files.

## Set up generation

Choose typed bindings when initializing a declarative project:

```sh
langmeld init my-language --typed-bindings
```

The initializer adds `langmeld-build` as a build dependency, creates `build.rs`,
includes the generated module from Cargo's `OUT_DIR`, and sets
`rust_bindings = "typed"` in `langmeld.toml`. The regular declarative starter
uses dynamic field lookup by default and does not need a build script.

Add `langmeld-build` as a build dependency:

```toml
[build-dependencies]
langmeld-build = "0.12"
```

Keep this on the same Langmeld release line as `langmeld-core`,
`langmeld-language`, and `langmeld-project`. The current `0.12.1` versions are
not available from crates.io yet. When initializing from a Langmeld source
checkout, accept the local-crates prompt or pass `--local-crates` with
`--typed-bindings`; this configures all Langmeld dependencies to use that same
checkout. See the [project workflow](../guides/project-workflow.md#create-a-project).

Create `build.rs` at the Rust package root:

```rust,ignore
fn main() {
    if let Err(error) = langmeld_build::generate_project() {
        panic!("{error}");
    }
}
```

The generator reads `langmeld.toml` from `CARGO_MANIFEST_DIR`, loads the root
definition and its imports, and writes `langmeld_bindings.rs` to Cargo's
`OUT_DIR`. It emits Cargo rebuild instructions for the manifest, loaded
definition files, and narrowly scoped paths needed to detect in-project
symlink imports. It avoids watching the project root recursively.

Keep a stable module in the Rust source tree and include the build output:

```rust,ignore
pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/langmeld_bindings.rs"));
}
```

The generated file belongs to Cargo's build output. It should not be copied
into `src/` or committed.

For a Rust-authored grammar, call `langmeld_build::generate_bindings(&spec)` in
the build script and write the returned source to `OUT_DIR`. This uses the same
projection generator without requiring `langmeld.toml`.

## Use typed captures

Generated construct views expose one method per declared capture. Identifiers
return `CapturedIdentifier`, preserving the spelling and source span. Other
capture kinds return `CapturedText` with their exact source text and span. Each
construct view also exposes its declared construct name through `NAME`.

```rust,ignore
use langmeld_language::{ConstructContext, ConstructHandler, HandlerControl};

pub struct PrintHandler;

impl ConstructHandler<RuntimeState> for PrintHandler {
    const CONSTRUCT: &'static str = crate::generated::constructs::Print::NAME;

    fn handle(
        &self,
        context: &mut ConstructContext<'_, RuntimeState>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        let print = context.typed::<generated::constructs::Print>()?;
        let name = print.name()?;

        context.state_mut().output.push_str(name.text);
        Ok(HandlerControl::Continue)
    }
}
```

Misspelled or undeclared accessor names fail Rust compilation. At runtime, the
view checks that the parsed construct and syntax variant match the generated
definition; incompatible captures return a source-located diagnostic.
Typed handler scaffolds also derive their dispatch name from that generated
construct type, so the registration identity cannot drift from the view used
to access its captures. Dynamic handlers continue to declare their construct
name directly.
The view validates the construct's capture shape as it is accessed; it does
not fingerprint the entire `LanguageSpec`. A different grammar with a
compatible construct and capture shape can use the same projection. The Rust
host remains responsible for deciding what those captures mean.

## Named groups and syntax variants

Named optional groups generate `Option`-returning accessors. Named repeated
groups generate `Vec`-returning accessors. Each group view has methods only for
the captures declared inside that group, so fields from different repetitions
stay paired:

```rust,ignore
let function = context.typed::<generated::constructs::Function>()?;

if let Some(parameters) = function.parameters()? {
    for parameter in parameters.additional_parameters()? {
        let name = parameter.parameter()?;
        let value_type = parameter.parameter_type()?;
    }
}
```

When a construct has multiple syntax alternatives, name every alternative with
`syntax "name"`. The generated construct type is then an enum whose variants
contain variant-specific capture views. Optional and repeated groups must also
be named. This keeps generated Rust names stable when unrelated syntax is
added.

Capture names are converted to Rust `snake_case` method names and construct
names to `UpperCamelCase` types. If conversion would create invalid or
colliding Rust names, generation fails with an actionable error. Use the
dynamic `ConstructContext` helpers when a grammar intentionally uses names that
cannot be represented as Rust identifiers, or when generated views are not
needed. For single-variant constructs, a top-level capture or group that maps
to `from_construct` conflicts with the typed-view constructor and is rejected;
rename that capture or use the dynamic API.

See the [`declarative-functions` example](../../examples/declarative-functions/README.md)
for a complete build script and generated views used by a language-specific
AST lowering stage.
