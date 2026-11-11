# Examples

Each example focuses on a different way to use Langmeld. Start with
[`toy-language`](toy-language) for the standard declarative workflow, then use
the other examples to compare parser and semantic extension points.

| Example | Use it to explore | Main Langmeld capabilities |
| --- | --- | --- |
| [`toy-language`](toy-language) | A working language with grammar and Rust behavior | Modular `.langmeld`, construct handlers, expression parsing, host-defined runtime |
| [`brainfuck`](brainfuck) | A complete interpreter for an existing language | Custom lexer, custom AST/parser, bounded execution, source diagnostics |
| [`declarative-functions`](declarative-functions) | Function declarations using the built-in parser | Parameter captures, typed AST lowering, ordered signature and call analysis |
| [`custom-lexer`](custom-lexer) | Nonstandard tokens with the generic parser | Custom lexer, token-span validation, declarative grammar |
| [`aster-language`](aster-language) | Rust-like functions with parameters, scopes, and semantic analysis | Custom parser, typed AST, ordered analysis passes, symbols, diagnostics, LSP navigation |
| [`schema-language`](schema-language) | Semantics spanning multiple source files | Workspace analysis, cross-file types, semantic completion and navigation |
| [`embedded-definitions`](embedded-definitions) and [`geometry-extension`](geometry-extension) | Composing definitions supplied by a Rust crate | In-memory definition sources, overlay resolver, explicit imports |

Each example covers a different authoring need: the toy language demonstrates
the standard declarative runtime path; Brainfuck demonstrates a complete,
bounded custom-language implementation; and Aster and Schema demonstrate
language-owned semantic and editor features. `declarative-functions` and Aster
express functions through different syntax ownership paths, showing where the
generic parser is sufficient and where a custom parser is useful. The examples
remain separate so each language's syntax, semantics, and runtime policy stay
easy to follow.

Run the examples from the workspace root. Their individual READMEs describe
the sample language, commands, and limits in more detail.
