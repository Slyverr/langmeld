# langmeld-tree-sitter

Generate a Tree-sitter `grammar.js` file from a declarative Langmeld
`LanguageSpec`.

The exporter maps construct variants, captured fields, optional and repeated
groups, and delimited child blocks. It provides lexical rules for identifiers,
decimal integers, quoted strings, and grouped punctuation.

```rust,ignore
use langmeld_tree_sitter::{TreeSitterOptions, generate};

let grammar_source = generate(&language, &TreeSitterOptions::new("my_language"))?;
```

For a manifest-backed project, use the CLI:

```sh
langmeld generate tree-sitter \
  --manifest-path langmeld.toml \
  --grammar-name my_language > grammar.js
tree-sitter generate grammar.js
```

The Tree-sitter CLI checks grammar conflicts and produces the parser. Langmeld
does not silently resolve conflicts on behalf of a language author. The
exporter cannot infer a host-defined lexer, custom parser, semantic behavior,
runtime, parser bindings, or highlighting queries. Those parts remain
language-owned.
