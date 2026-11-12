# Examples

Start with [`toy-language`](toy-language) for the normal declarative workflow:
modular `.langmeld` syntax, Rust construct handlers, and a runnable host.

## Complete implementation of an existing language

[`brainfuck`](brainfuck) implements the complete eight-command Brainfuck
language as an interpreter. It demonstrates a custom lexer and parser,
source-located diagnostics, execution limits, and runtime behavior owned by the
language host.

## Larger language consumers

| Example | What it demonstrates |
| --- | --- |
| [`aster-language`](aster-language) | A custom parser and AST, ordered semantic passes, lexical scopes, LSP navigation, and a context-limited function snippet |
| [`schema-language`](schema-language) | Workspace-wide analysis, cross-file references, semantic completion, and LSP integration |

## Focused capabilities

| Example | What it demonstrates |
| --- | --- |
| [`declarative-functions`](declarative-functions) | Parameterized declarations with the generic parser and host-owned analysis |
| [`custom-lexer`](custom-lexer) | A custom lexer while retaining the generic `.langmeld` parser |
| [`embedded-definitions`](embedded-definitions) and [`geometry-extension`](geometry-extension) | A consumer combining local definitions, a crate-provided extension, and a Rust-authored construct |

Each example focuses on a distinct authoring path or framework capability.
Language-specific semantics and runtime policy remain in the host project; no
single sample needs every Langmeld feature. Run commands and scope notes are in
each example's README.
