# Language definitions

Langmeld's `.lm` frontend currently describes token-pattern grammars. It is a
small declarative layer over the validated Rust grammar model, not a complete
grammar notation or a language runtime. Hosts keep control of lexing, parsing,
semantics, and execution when the declarative subset is not a fit.

## Project layout

Keep a root language file and split constructs into files by responsibility:

```text
language/
├── language.lm
└── constructs/
    ├── statements.lm
    └── expressions.lm
```

The root file declares the language and imports construct files. Imported
files contain top-level `construct` and `import` declarations; they do not
declare another language. Imports are explicit and may be nested.

```lm
language "WidgetScript" {
    import "constructs/statements.lm";

    construct "program" {
        syntax {
            literal "program";
        }
        block "{" "}" {
            allow "let";
            allow "draw";
        }
    }
}
```

`constructs/statements.lm` can import `expressions.lm` and declare constructs
used by the root block. See the complete consumer in
[`examples/toy-language`](../examples/toy-language).

## Syntax patterns

Every construct has at least one `syntax` block. Multiple blocks are
alternatives for the same construct; a parsed node records the zero-based
alternative index in `variant`.

```lm
construct "function" {
    syntax {
        literal "fn";
        identifier "name";
        literal "(";
        optional {
            identifier "parameter";
        }
        literal ")";
        literal "->";
        identifier "return_type";
    }
    syntax {
        literal "function";
        identifier "name";
    }
}
```

Pattern items match lexer tokens in order:

- `literal "...";` matches an exact token.
- `token "field";` captures any valid token from the lexer and preserves its
  text and `TokenKind` in the parsed field.
- `identifier "field";`, `integer "field";`, and `string "field";`
  capture one token of that lexical category under a field name.
- `optional { ... }` matches its sequence zero or one times.
- `zero_or_more { ... }` and `one_or_more { ... }` repeat a sequence; fields
  from each match are captured in source order.

Captured values preserve their lexical text. For example, integer captures
are not parsed into numeric values and strings retain their quotes. Hosts
decide how to validate or interpret them. `token` is useful for a host lexer
that recognizes a language-specific form; it accepts any non-invalid token
and leaves classification or validation to the host.

The definition validator rejects duplicate names, invalid literals, empty or
nullable patterns, and ambiguous syntax boundaries it can identify. It does
not implement arbitrary CFG/PEG/regex grammar features. In particular, the
generic parser is not an expression parser: use `langmeld-expression` for its
supported expression forms or write a host parser for another grammar.

## Blocks

A construct may declare one delimited block. The delimiters are exact lexer
tokens, and `allow` lists the construct names permitted as direct children:

```lm
construct "conditional" {
    syntax {
        literal "if";
        identifier "condition";
    }
    block "{" "}" {
        allow "draw";
    }
}
```

This is a parser boundary, not a semantic scope. Langmeld does not infer
variable visibility, declaration order, name resolution, or control-flow
meaning from blocks. The generic parser limits block nesting to 128 levels.

## Loading a project

`parse_language_definition` handles one source file and reports an error if
that file contains imports. For a multi-file project, pass a root source and a
host implementation of `DefinitionResolver` to
`parse_language_definition_project`:

```rust,ignore
use langmeld_core::SourceFile;
use langmeld_language::{
    DefinitionResolver, DefinitionSource, parse_language_definition_project,
};

struct ProjectResolver;

impl DefinitionResolver for ProjectResolver {
    fn resolve(
        &self,
        from_key: &str,
        import: &str,
    ) -> Result<DefinitionSource, String> {
        // Resolve relative to `from_key`, enforce the project's allowed root,
        // read the source, then return a stable key and named snapshot.
        unimplemented!("load the requested source from the host project")
    }
}

let root = DefinitionSource::new(
    "language/language.lm",
    SourceFile::new("language/language.lm", root_text),
);
let result = parse_language_definition_project(root, &ProjectResolver);
```

Langmeld does not access the filesystem or prescribe path rules. The resolver
owns normalization, project-root restrictions, source loading, and stable
keys. Keys identify logical sources for de-duplication and cycle detection.
Import cycles are errors, repeated imports of an already loaded key are
ignored, and import nesting is bounded at 64 sources. Diagnostics retain the
source name and span that produced each error. The Toy example uses a small
filesystem resolver that keeps imports inside its language project and retains
loaded snapshots for line-and-column diagnostics.

## When to use Rust hooks

Use `.lm` when the language can be described by ordered token patterns and
explicit child blocks. Provide a custom `Lexer` when tokenization differs from
the built-in identifiers, decimal integers, quoted strings, and punctuation.
Implement `LanguageParser` when syntax requires a different grammar strategy
or the host needs its own AST. The host parser contract returns the host tree,
source identity, and diagnostics; it does not automatically provide shared
editor tooling for that tree.

For optional semantic analysis, implement `LanguageAnalyzer<P>` for the parser.
It receives the same source snapshot and parsed document, then returns an
`AnalysisDocument<T>` containing the source identity, a host-defined result, and
semantic diagnostics. The host chooses its analysis phases and policies;
languages that do not need semantic analysis can use the parser alone.

## Syntax completions

`complete_at` returns grammar-derived construct-start tokens accepted at a byte
offset. Suggestions follow the current root or nested block scope, filter by a
partially typed identifier, include the active block's closing delimiter, and
provide a replacement range. Use `complete_at_with_lexer` with a custom lexer.
These completions come from declared syntax only; field values and semantic
suggestions require host-provided analysis. Custom parsers do not receive these
grammar-derived suggestions.
