# langmeld-textmate

Generates a lexical TextMate grammar from a declarative `LanguageSpec`.
Language authors provide a unique scope name and may provide file extensions.

The output highlights grammar literal words and punctuation, decimal integers,
double-quoted strings, and configured line and block comments. It does not
reproduce parser structure, infer semantic roles, or represent host-defined
lexer rules. Extend the generated grammar for other language-specific lexical
behavior.

See the [editor integration guide](../../docs/editor-integration.md) for how
this output fits with LSP-based language features.
