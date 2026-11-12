# Tree-sitter grammar export

The optional `langmeld-tree-sitter` crate converts a declarative `LanguageSpec`
to a Tree-sitter `grammar.js` source. It maps construct variants, captures,
optional and repeated groups, and child blocks. The generated lexer covers the
configured line comments and the built-in identifier, decimal integer, quoted
string, and punctuation categories. Configured block comments are reported as
unsupported. Escaped line breaks in strings follow the built-in Langmeld
lexer. Punctuation follows its grouped ASCII operator runs and single-character
fallback tokens.
It cannot infer a host lexer, custom parser, semantic rules, or runtime
behavior, so those languages need a host-authored Tree-sitter grammar.

Generate a grammar from a project manifest with:

```sh
langmeld generate tree-sitter \
  --manifest-path langmeld.toml \
  --grammar-name my_language > grammar.js
```

The Tree-sitter CLI is required to validate and compile the generated grammar:
run `tree-sitter generate grammar.js` from the grammar project. Tree-sitter can
reject conflicts that Langmeld's generic grammar model cannot resolve; edit
the generated grammar or keep a host-authored grammar for those cases. The
exporter does not generate a Tree-sitter project, parser bindings, or
highlighting queries.
