# Examples

Each example focuses on a different way to use Langmeld. Start with
[`toy-language`](toy-language) for the standard declarative workflow, then use
the other examples to compare parser and semantic extension points.

| Example | Use it to explore | Main Langmeld capabilities |
| --- | --- | --- |
| [`toy-language`](toy-language) | A working language with grammar and Rust behavior | Modular `.langmeld`, construct handlers, expression parsing, host-defined runtime |
| [`declarative-functions`](declarative-functions) | Function declarations using the built-in parser | Parameter captures, typed AST lowering, ordered signature and call analysis |
| [`custom-lexer`](custom-lexer) | Nonstandard tokens with the generic parser | Custom lexer, token-span validation, declarative grammar |
| [`custom-parser`](custom-parser) | A small language whose syntax is written in Rust | `LanguageParser`, host-owned AST, reusable expression parser |
| [`aster-language`](aster-language) | Rust-like functions with parameters, scopes, and semantic analysis | Custom parser, typed AST, ordered analysis passes, symbols, diagnostics, LSP navigation |
| [`schema-language`](schema-language) | Semantics spanning multiple source files | Workspace analysis, cross-file types, semantic completion and navigation |
| [`embedded-definitions`](embedded-definitions) and [`geometry-extension`](geometry-extension) | Composing definitions supplied by a Rust crate | In-memory definition sources, overlay resolver, explicit imports |

The examples keep the declarative parser and custom-parser route distinct.
`declarative-functions` and Aster express a similar function concept through
different syntax ownership paths; compare them to see when `.langmeld` is
sufficient and when a custom parser is useful. Aster and Schema also use
different semantic models to show that scope, type, and workspace policies
belong to the language host. Together, the catalog covers the framework's
current extension points without suggesting that one sample language needs
every feature.

Run the examples from the workspace root. Their individual READMEs describe
the sample language, commands, and limits in more detail.
