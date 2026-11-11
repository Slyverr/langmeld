# Construct handlers

Construct handlers attach Rust behavior to stable construct names declared in
`.langmeld`. They are a low-level dispatch hook, not a required interpreter or
runtime model. Each language chooses its own state and decides what constructs
mean.

## Generate a handler

Declare a construct in the language definition, then run this from the
initialized language project:

```sh
langmeld generate handler print
```

The command validates the project and construct name, creates
`src/constructs/print.rs`, and adds its module and registration to the managed
sections in `src/constructs/mod.rs`. It refuses to overwrite an existing
handler. The generated implementation returns a source-located diagnostic
until you add the language-specific behavior.

The command maps construct names to Rust module names. Names containing only
ASCII letters, digits, hyphens, and underscores are supported; Rust keywords
receive a safe module-name suffix. Ambiguous or invalid names are rejected.

## Implement behavior

`ConstructHandler<State>` receives the parsed construct and a context holding
the host's state. The context provides named-field lookup, checked identifier
access, source text for captures, source-located diagnostics, and explicit
dispatch of child constructs.

```rust,ignore
use langmeld_core::Diagnostic;
use langmeld_language::{
    ConstructContext, ConstructHandler, HandlerControl, ParsedConstruct,
};

use crate::runtime::RuntimeState;

pub(super) struct PrintHandler;

impl ConstructHandler<RuntimeState> for PrintHandler {
    const CONSTRUCT: &'static str = "print";

    fn handle(
        &self,
        construct: &ParsedConstruct,
        context: &mut ConstructContext<'_, RuntimeState>,
    ) -> Result<HandlerControl, Diagnostic> {
        let field = context.field(construct, "name")?;
        let name = context.field_identifier(construct, "name")?.to_owned();
        let value = context
            .state()
            .variables
            .get(&name)
            .cloned()
            .ok_or_else(|| {
                context.error_at(
                    "EXAMPLE-001",
                    format!("unknown variable `{name}`"),
                    field.span,
                )
            })?;

        context.state_mut().output.push_str(&value);
        context.state_mut().output.push('\n');

        Ok(HandlerControl::Continue)
    }
}
```

The `State` type is supplied by the host; Langmeld does not require a symbol
table, value type, output stream, or runtime context. Field helpers report
missing fields, wrong capture kinds, and invalid spans as source-located
diagnostics instead of requiring unchecked indexing.

## Child constructs and control flow

`dispatch_block` visits a block's direct children in source order. It does not
walk nested children automatically. A handler calls `context.dispatch_children`
when its semantics require it—for example, after a condition succeeds or on
each loop iteration.

Handlers can use a host-defined signal type to propagate `return`, `break`, or
other control outcomes. The default signal type is `()`, so a simple language
can use `HandlerControl::Continue` without defining control flow. Langmeld
does not prescribe what a signal means; the enclosing handler or host decides.

Handlers are registered explicitly and duplicate construct names are rejected.
Dispatching an unregistered construct returns a source-located diagnostic.
Registration order does not change execution order; source order controls
block dispatch.

## Beyond handlers

Handlers work well for local behavior such as storing a binding or emitting one
statement. Whole-language rules such as collecting declarations, resolving
references, or checking types belong in analysis passes. Those pass and backend
interfaces are separate roadmap work; this handler API does not impose an
interpreter or compiler pipeline.

For a complete runnable starter, see the generated project and
[project workflow](../guides/project-workflow.md). For syntax composition, see
[language definitions](definitions.md).
