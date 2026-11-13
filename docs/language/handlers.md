# Construct handlers

Construct handlers attach Rust behavior to stable construct names declared in
`.langmeld`. They are a low-level dispatch hook, not a required interpreter or
runtime model. Each language chooses its own state and decides what constructs
mean.

## Generate a construct and handler

For the common workflow, add a new construct and handler from the initialized
language project. In an interactive terminal you can omit the name and answer
the prompts for its stable identity and initial syntax spelling. The spelling
defaults to the construct name, and scripts keep that same deterministic
default unless `--literal` is supplied:

```sh
# Interactive: prompts for the construct name and starter spelling.
langmeld generate construct
```

This creates `language/constructs/print.langmeld`, adds an explicit import to
the configured root definition, and creates and registers
`src/constructs/print.rs`. If the language has more than one block category,
the command prompts in an interactive terminal; in scripts, choose one with
`--category <name>`. The construct name is its stable semantic identity; the
starter syntax literal defaults to the same text. Use `--literal` when the
spelling differs in a script:

```sh
langmeld generate construct function --literal fn
```

This creates a `function` construct handled by Rust's `FunctionHandler`, with
`fn` as its initial syntax literal. Edit the generated definition for compound
or alternative syntax. Existing files are never overwritten.

To generate behavior for syntax that already exists in the definition, run:

```sh
langmeld generate handler print
```

The command validates the project and construct name, creates
`src/constructs/print.rs`, and adds its module and registration to the managed
sections in `src/constructs/mod.rs`. It refuses to overwrite an existing
handler. The generated implementation returns a source-located diagnostic
until you add the language-specific behavior.

The command maps stable construct names to Rust module names. Names containing
ASCII letters, digits, hyphens, and underscores are supported; Rust keywords
receive a safe module-name suffix. Names that map to an existing Rust module
are rejected.

## Implement behavior

`ConstructHandler<State>` receives a context bound to the current parsed
construct and the host's state. The context provides direct access to that
construct, named-field lookup, checked identifier access, source text for
captures, source-located diagnostics, and explicit dispatch of child
constructs.

```rust,ignore
use langmeld_core::Diagnostic;
use langmeld_language::{
    ConstructContext, ConstructHandler, HandlerControl,
};

use crate::runtime::RuntimeState;

pub(super) struct PrintHandler;

impl ConstructHandler<RuntimeState> for PrintHandler {
    const CONSTRUCT: &'static str = "print";

    fn handle(
        &self,
        context: &mut ConstructContext<'_, RuntimeState>,
    ) -> Result<HandlerControl, Diagnostic> {
        let name = context.field_identifier_capture("name")?;
        let value = context
            .state()
            .variables
            .get(name.text)
            .cloned()
            .ok_or_else(|| {
                context.error_at(
                    "EXAMPLE-001",
                    format!("unknown variable `{}`", name.text),
                    name.span,
                )
            })?;

        context.state_mut().output.push_str(&value);
        context.state_mut().output.push('\n');

        Ok(HandlerControl::Continue)
    }
}
```

The `State` type is supplied by the host; Langmeld does not require a symbol
table, value type, output stream, or runtime context. The identifier helper
returns captured text and its source span together, but resolution remains the
language host's responsibility. Field helpers report missing fields, wrong
capture kinds, and invalid spans as source-located diagnostics instead of
requiring unchecked indexing.

Registered handlers can borrow configuration for as long as the registry is
used; they do not need to be `'static`. Keep mutable shared execution state in
`State`, where handlers access it through the context. If a function returns a
registry whose handlers borrow from `config`, name that lifetime in the return
type as `ConstructHandlerRegistry<'config, State>`; local registry types are
inferred. For example, a language can keep configuration separate from
per-execution state and return a registry that borrows the configuration:

```rust,ignore
use langmeld_core::Diagnostic;
use langmeld_language::{
    ConstructContext, ConstructHandler, ConstructHandlerRegistry, HandlerControl,
    HandlerRegistrationError,
};

struct PrefixHandler<'config> {
    prefix: &'config str,
}

impl ConstructHandler<Vec<String>> for PrefixHandler<'_> {
    const CONSTRUCT: &'static str = "greet";

    fn handle(
        &self,
        context: &mut ConstructContext<'_, Vec<String>>,
    ) -> Result<HandlerControl, Diagnostic> {
        let name = context.field_identifier("name")?;

        context
            .state_mut()
            .push(format!("{} {name}", self.prefix));

        Ok(HandlerControl::Continue)
    }
}

fn greeting_handlers<'config>(
    prefix: &'config str,
) -> Result<ConstructHandlerRegistry<'config, Vec<String>>, HandlerRegistrationError> {
    let mut handlers = ConstructHandlerRegistry::new();

    handlers.register(PrefixHandler { prefix })?;

    Ok(handlers)
}
```

The handler retains the borrow as long as the registry lives, so the
configuration does not need to be cloned. The integration test for borrowed
handlers covers the same return-type pattern.

Field names passed to these helpers are runtime lookups. Rust does not check
them against the `.langmeld` definition at compile time, so a misspelled or
undeclared name is reported only when the handler accesses that field.
Projects that want compile-time checked capture access can use the optional
[`langmeld-build` typed bindings](typed-bindings.md).

In typed mode, implement `TypedConstructHandler` and register it with
`ConstructHandlerRegistry::register_typed`. Its associated generated view
provides the dispatch name, and Langmeld passes that view into the handler.
This keeps the construct identity and typed projection together. Runtime-loaded
definitions are still validated when the view is created and produce a
source-located diagnostic if their captures do not match.

When a grammar can capture the same field more than once, `field` and
`field_text` return the first capture. Use `fields` to process all captures in
source order; it returns an empty iterator when an optional or repeated group
produced no capture. `captured_text` reads the original source for a particular
capture:

```rust,ignore
for field in context.fields("message") {
    let message = context.captured_text(field)?;
    // Interpret the captured text according to this language's rules.
}
```

When one repeated item contains multiple fields that belong together, inspect
`context.construct().capture_groups`. Each `CaptureGroupOccurrence` returns the
flat fields captured by that one match, so handlers can process a parameter's
name and type together instead of independently zipping two field lists.

The `emit` handler in the
[`toy-language` example](../../examples/toy-language/README.md) shows repeated
captures parsed from a real `.langmeld` definition and used by Rust behavior.

## Child constructs and control flow

`dispatch_block` visits a block's direct children in source order. It does not
walk nested children automatically. A handler calls `context.dispatch_children()`
when its semantics require it—for example, after a condition succeeds or on
each loop iteration.

If top-level constructs have behavior of their own, register their handlers
and start with `registry.dispatch_parse_result(source, &parsed, &mut state)`.
This dispatches the top-level constructs in source order and checks that the
parse result belongs to the supplied source snapshot. It does not dispatch
nested children automatically; each handler can call
`context.dispatch_children()` when its semantics require it. Check
`parsed.is_valid()` first when execution should require a fully valid parse.

Use `dispatch_construct` when intentionally dispatching one construct, and
`dispatch_block` when the block itself is only a structural container. Pass
both the `ParseResult` and a construct borrowed from it; Langmeld validates the
source identity and rejects a node from a different parse result. For example:

```rust,ignore
let parsed = parse(source, &language_spec);
let program = &parsed.constructs()[0];

registry.dispatch_block(source, &parsed, program, &mut state)?;
```

The usual `dispatch_parse_result` and `dispatch_top_level_children` entry
points already receive the complete parse result and do not require selecting a
construct manually.

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
references, or checking types belong in analysis passes. Use
[ordered analysis passes](analysis-passes.md) when those rules need shared
facts or distinct stages. The host chooses how parsing, analysis, and handler
execution fit together; Langmeld does not impose an interpreter or compiler
pipeline.

### Interpreters and runtimes

An **interpreter** is an execution engine that runs a program representation
directly, such as a Langmeld parse tree, a host-owned AST, or an IR. A
**language runtime** is the state and services the program uses while running,
such as values, memory, built-ins, or I/O. These are separate roles: an
interpreter uses or manages runtime state, though a small host may implement
both in one Rust type. A bytecode VM is an interpreter for bytecode. A native
executable may include or link language-runtime support, such as a garbage
collector or standard library, and still relies on operating-system services.

### Compilers and targets

A **compiler** translates a program representation into another form, such as
bytecode, WebAssembly, or native code. Compiling and running the result are
separate steps: bytecode needs a VM, WebAssembly needs a compatible engine, and
native output needs linking and may depend on runtime libraries.

Langmeld supplies no universal AST, IR, runtime, interpreter, compiler, or
`langmeld build` command. The language host chooses whether to use Langmeld's
parsed constructs or lower them into its own AST/IR, and owns the language's
semantics, execution, code generation, and targets. A host can share its
parser, analysis, and IR between an interpreter and compiler when that fits the
language. `cargo build` builds the Rust host executable; it does not compile
programs written in the language that host implements.

### Examples

These examples show distinct host-owned choices:

- [`toy-language`](../../examples/toy-language/README.md) interprets
  declaratively parsed constructs through Rust handlers and `RuntimeState`.
- [Brainfuck](../../examples/brainfuck/README.md) uses a custom parser to build
  a nested AST, then executes it with a bounded Rust runtime.
- [Declarative Functions](../../examples/declarative-functions/README.md)
  lowers a declarative grammar into a typed AST and runs analysis passes; it
  does not add an interpreter or compiler.
- [Aster](../../examples/aster-language/README.md) uses a custom parser, AST,
  and semantic analysis, with no execution backend.

For a complete runnable starter, see the generated project and
[project workflow](../guides/project-workflow.md). For syntax composition, see
[language definitions](definitions.md).
