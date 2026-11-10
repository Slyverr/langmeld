# Language definitions

Langmeld's `.langmeld` frontend currently describes token-pattern grammars. It is a
small declarative layer over the validated Rust grammar model, not a complete
grammar notation or a language runtime. Hosts keep control of lexing, parsing,
semantics, and execution when the declarative subset is not a fit.

## Project layout

Keep a root language file and split constructs into files by responsibility:

```text
language/
├── language.langmeld
└── constructs/
    ├── statements.langmeld
    └── expressions.langmeld
```

The root file declares the language and imports construct files. Imported
files contain top-level `construct` and `import` declarations; they do not
declare another language. Imports are explicit and may be nested.

```langmeld
language "WidgetScript" {
    import "constructs/statements.langmeld";
    lexical {
        line_comment "//";
    }

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

`constructs/statements.langmeld` can import `expressions.langmeld` and declare constructs
used by the root block. See the complete consumer in
[`examples/toy-language`](../examples/toy-language).

## Syntax patterns

Every construct has at least one `syntax` block. Multiple blocks are
alternatives for the same construct; a parsed node records the zero-based
alternative index in `variant`.

```langmeld
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
- `expression "field" until ";";` captures the source span up to a top-level
  terminator and consumes that terminator. Nested `()`, `[]`, and `{}` groups
  may contain the same token without ending the capture.
- `optional { ... }` matches its sequence zero or one times.
- `zero_or_more { ... }` and `one_or_more { ... }` repeat a sequence; fields
  from each match are captured in source order.

## Lexical comments

The built-in lexer accepts optional line and non-nested block-comment markers.
Declare them once in the root language file:

```langmeld
language "WidgetScript" {
    lexical {
        line_comment "//";
        block_comment "/*" "*/";
    }

    // constructs and imports go here
}
```

Comments are skipped by the built-in parser and completion lexer, while text
inside quoted strings remains unchanged. Unterminated block comments produce a
source-located lexical diagnostic. The built-in meta-grammar also accepts `//`
comments in `.langmeld` files. A host lexer supplied through
`parse_with_lexer` owns its own comment handling. Hosts that parse a captured
expression span must also teach their expression parser about comments inside
that span.

TextMate export includes configured line and block comments. Tree-sitter export
includes line comments, but reports block comments as unsupported so it does
not emit a grammar with misleading behavior.

Captured values preserve their lexical text. For example, integer captures
are not parsed into numeric values and strings retain their quotes. Hosts
decide how to validate or interpret them. `token` is useful for a host lexer
that recognizes a language-specific form; it accepts any non-invalid token
and leaves classification or validation to the host.

The definition validator rejects duplicate names, invalid literals, empty or
nullable patterns, and ambiguous syntax boundaries it can identify. It does
not implement arbitrary CFG/PEG/regex grammar features. Expression captures
only identify a bounded source span; the host still chooses how to parse,
validate, and interpret it. Use `langmeld-expression` for its supported
expression forms or write a host parser for another grammar. The Tree-sitter
exporter reports these host-parsed captures as unsupported because it cannot
infer the embedded expression grammar.

## Blocks

A construct may declare one delimited block. The delimiters are exact lexer
tokens. `allow` lists construct names permitted as direct children. When
several blocks share the same child group, constructs can implement a named
category and a block can allow that category:

```langmeld
construct "conditional" {
    syntax {
        literal "if";
        identifier "condition";
    }
    block "{" "}" {
        allow_category "statement";
    }
}

construct "draw" {
    syntax {
        literal "draw";
        identifier "shape";
        literal ";";
    }
    implements "statement";
}
```

The first `implements` membership implicitly introduces the category; there is
no separate category declaration to keep in sync. Imported constructs can
implement a category used by the root definition. A block may combine `allow`
and `allow_category`; direct entries are kept first, then category members are
added in language declaration order, with overlapping entries included once.
An unknown or empty category is a validation error.

Categories only group grammar constructs. They do not infer variable
visibility, declaration order, name resolution, or control-flow meaning from
blocks.

## Parser resource limits

The generic parser defaults to a 4 MiB source limit, 250,000 lexer tokens,
250,000 parsed constructs, 128 nested blocks, and 1,000,000 pattern-matching
steps. `ParseResult::resource_limited` indicates that one of these bounds
limited the result; completed constructs are retained where possible.

Raise or lower these limits per parse with `ParseLimits`:

```rust,ignore
use langmeld_language::{ParseLimits, parse_with_limits};

let result = parse_with_limits(
    &source,
    &language,
    ParseLimits {
        max_source_bytes: 8 * 1024 * 1024,
        ..ParseLimits::default()
    },
);
```

`parse_with_lexer_and_limits` applies the same parser bounds with a host lexer.
The generic parser checks source size before calling the lexer and caps the
tokens it validates. A custom lexer that returns more than the token limit is
discarded by default, but this post-check cannot bound the lexer's own work or
allocations. Override `Lexer::lex_with_token_limit` to stop early and return a
valid prefix. These limits do not constrain custom `LanguageParser`
implementations, which own their parsing behavior and must choose their own
resource policy.

The expression parser defaults to a 4 MiB selected-range limit, 250,000 tokens,
and 128 levels of expression-tree depth. Configure these limits when creating
the parser:

```rust,ignore
use langmeld_expression::{ExpressionLimits, ExpressionParser};

let parser = ExpressionParser::with_limits(
    &grammar,
    ExpressionLimits {
        max_tokens: 50_000,
        ..ExpressionLimits::default()
    },
);
let result = parser.parse(&source, range);
```

`ExpressionParseResult::resource_limited` distinguishes a truncated token stream
or depth-limited parse. The expression parser caps configured expression-tree
depth at 128, including long member or operator chains; host parsers that do not
use it own their resource policy.

## Loading a project

`parse_language_definition` handles one source file and reports an error if
that file contains imports. For a normal manifest-backed filesystem project,
`langmeld-project` loads the manifest, resolves imports, and returns the parsed
definition together with all source snapshots:

```rust,ignore
use langmeld_project::load_definition_project;

let project = load_definition_project(".")?;
if !project.is_valid() {
    for diagnostic in &project.definition.diagnostics {
        eprintln!("{}: {}", diagnostic.code, diagnostic.message);
    }
}
let language = project.definition.language;
```

Definition syntax and validation errors stay in the returned diagnostics so
the caller can render them with the included `project.sources`. File-system
and manifest failures are returned as errors. Use `load_definition_project_from_manifest`
when the manifest is not at the project root.

For custom source storage or editor overlays, implement `DefinitionResolver`
directly or use `DirectoryDefinitionResolver`, which confines imports to the
selected root and retains source snapshots:

```rust,ignore
use langmeld_language::parse_language_definition_project;
use langmeld_project::DirectoryDefinitionResolver;

let resolver = DirectoryDefinitionResolver::new(".")?;
let root = resolver.load("language/language.langmeld")?;
let result = parse_language_definition_project(root, &resolver);
let diagnostic_sources = resolver.sources();
```

The parser remains independent of filesystem access. Hosts that need another
storage model can implement `DefinitionResolver`; keys identify logical
sources for de-duplication and cycle detection. Import cycles are errors,
repeated imports of an already loaded key are ignored, and import nesting is
bounded at 64 sources. Diagnostics retain the source name and span that
produced each error. `DirectoryDefinitionResolver` accepts relative imports,
including parent segments when the canonical target remains within the project
root; it rejects absolute paths, backslash separators, and symlink escapes.

## When to use Rust hooks

Use `.langmeld` when the language can be described by ordered token patterns and
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

For host-owned trees, implement `LanguageCompleter<P>` to provide
language-specific completion candidates. Its callback receives the current
source, the existing parsed document, an optional analysis document, and a byte
offset. `complete_with` rejects parse or analysis documents from another
database-managed source revision and filters candidates whose replacement
spans are outside the source or split a UTF-8 character. Detached
`SourceFile`s have no revision identity, so hosts must pair their parse and
analysis results with the matching text. Use `type Analysis = ()` when
completion does not need semantic analysis. This keeps completion behavior
with the host language while allowing an editor integration to reuse its
current parse and analysis results.

For definition and references queries, implement `LanguageNavigator<P>`. The
host receives the same parsed and optional analyzed snapshot and returns
`SourceLocation`s for the selected symbol. This supports different resolution
policies while keeping the result consumable by editor adapters. `definition_at`
and `references_at` reject stale database-managed snapshots, invalid query
offsets, malformed spans, and invalid locations in the current source. Hosts
remain responsible for validating locations in other source files. Detached
sources have no revision identity, as with completions.

## Syntax completions

`complete_at` returns grammar-derived construct-start tokens accepted at a byte
offset. Suggestions follow the current root or nested block scope, filter by a
partially typed identifier, include the active block's closing delimiter, and
provide a replacement range. Use `complete_at_with_lexer` with a custom lexer.
These completions come from declared syntax only; field values and semantic
suggestions require host-provided analysis. Custom parsers do not receive these
grammar-derived suggestions.
