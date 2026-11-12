# TextMate grammar export

The optional `langmeld-textmate` crate converts a declarative `LanguageSpec`
into a JSON TextMate grammar. Supply a unique scope name such as
`source.widget`; file extensions are optional metadata:

```rust,ignore
use langmeld_textmate::{TextMateOptions, generate};

let options = TextMateOptions::new("source.widget").file_types(["widget"]);
let grammar_json = generate(&language, &options)?;
```

The output highlights declared literal words and punctuation, decimal
integers, double-quoted strings, and comments configured in the language
definition. It is lexical highlighting, not a generated parser: undeclared
comments, semantic roles, and host-specific lexer rules are not inferred. CI
tokenizes the generated example grammar with VS Code's TextMate
engine and checks representative keyword, punctuation, number, string, escape,
and non-keyword scopes, including string escapes at line boundaries. This
validates the emitted grammar's behavior in the TextMate tokenizer; it does not
test VS Code extension packaging, themes, or editor UI. Use the LSP for semantic
diagnostics and navigation. See the
[VS Code syntax highlighting guide](https://code.visualstudio.com/api/language-extensions/syntax-highlight-guide)
for TextMate grammar integration details.
