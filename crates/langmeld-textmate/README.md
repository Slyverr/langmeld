# langmeld-textmate

Generates a lexical TextMate grammar from a declarative `LanguageSpec`.
Language authors provide a unique scope name and may provide file extensions.

The output highlights grammar literal words and punctuation, decimal integers,
and double-quoted strings. It does not reproduce parser structure, infer
comments or semantic roles, or represent host-defined lexer rules. Extend the
generated grammar for language-specific lexical behavior.

See the [editor integration guide](../../docs/editor-integration.md) for how
this output fits with LSP-based language features.
