# Examples

Each example is a separate consumer project and demonstrates a different
Langmeld integration.

| Example | What it demonstrates |
| --- | --- |
| [`declarative-interpreter`](../examples/declarative-interpreter/README.md) | Modular `.langmeld` definitions, a syntax-only subtraction extension with a separate Rust handler, generic symbol resolution, and a Rust interpreter. |
| [`aster-language`](../examples/aster-language/README.md) | A generated typed AST lowered into host-owned IR, an ordered constant-folding pass, function-local scopes, a host numeric type rule, interpretation, and an independently composed assertion extension. |
| [`rust-functions`](../examples/rust-functions/README.md) | A Rust-authored model for zero-argument and parameterized function signatures, with lowering to compilable Rust. |
| [`rust-source`](../examples/rust-source/README.md) | Host-owned Rust parsing through `syn`, mapped into Langmeld's generic and typed ASTs. |
| [`typescript-source`](../examples/typescript-source/README.md) | TypeScript Compiler API parsing through a persistent Bun worker and LSP source indexing. |
| [`python-source`](../examples/python-source/README.md) | CPython parsing through a persistent worker, including direct relative sibling imports and LSP indexing. |
| [`c-signatures`](../examples/c-signatures/README.md) | A declarative C prototype model, a Rust host parser, and Rust FFI output. |
| [`combined-brief`](../examples/combined-brief/README.md) | A Rust-authored base model composed with a modular `.langmeld` extension and a Rust Markdown renderer. |

Run each example's commands from its own directory. The independent manifests
and lockfiles exercise Langmeld as a consumer rather than as a workspace-only
crate. Python and TypeScript examples require their documented runtimes; see
the individual READMEs before running them.
