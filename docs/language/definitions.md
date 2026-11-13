# Language definitions

Langmeld's `.langmeld` frontend currently describes token-pattern grammars. It is a
small declarative layer over the validated Rust grammar model, not a complete
grammar notation or a language runtime. Hosts keep control of lexing, parsing,
semantics, and execution when the declarative subset is not a fit.

## Project layout

Use `language/main.langmeld` as the project's conventional root file and split
constructs into imported files by responsibility:

```text
language/
├── main.langmeld
└── constructs/
    ├── statements.langmeld
    └── expressions.langmeld
```

`main.langmeld` declares the language and imports construct files. Imported
files contain top-level `construct` and `import` declarations; they do not
declare another language or root constructs. Imports are explicit and may be
nested.

```langmeld
language "WidgetScript" {
    import "constructs/statements.langmeld";
    root "program";
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

`root "program";` restricts top-level parsing and completion to that construct.
Declare one `root` for each construct the language accepts at source level.
When no roots are declared, all constructs remain allowed at the root for
backward compatibility and formats with multiple top-level construct types.
When `root` declarations are present, if a block is missing its closing token
and the parser reaches a complete root construct that is not allowed in that
block, it reports the missing closer at the root boundary and continues parsing
the root. A construct that also parses as a valid child remains a child. The
`root` directive belongs in the root language file; its name can refer to a
construct declared in an imported file.

## Parsed syntax tree

The declarative parser returns both its `ParsedConstruct` hierarchy and a
lossless `SyntaxTree`. `SyntaxTree::root` groups source-file, construct, field,
and captured-expression nodes. Its flat token table retains lexical tokens,
configured comments, whitespace, line endings, and unclassified source text;
spans refer to the original `SourceFile` and can be sliced with
`SyntaxToken::text`.

```rust
let parsed = langmeld_language::parse(&source, &language);
let reconstructed = parsed
    .syntax_tree()
    .tokens
    .iter()
    .map(|token| token.text(&source).expect("token span belongs to source"))
    .collect::<String>();

assert_eq!(reconstructed, source.text());
```

The tree is useful for editor features and host transformations that need
source-preserving structure. It does not choose formatting rules or infer
language semantics. Custom `LanguageParser` implementations continue to return
their own tree through `ParsedDocument<T>`. If the source-size limit rejects
input before lexing, the tree contains one unknown token spanning the source.
Borrow parse output through its accessors, or consume `ParseResult` with
`into_parts()` to move its constructs, syntax tree, diagnostics, and source
identity without cloning.

`constructs/statements.langmeld` can import `expressions.langmeld` and declare constructs
used by the root block. See the complete consumer in
[`examples/toy-language`](../../examples/toy-language).

## Syntax patterns

Every construct has at least one `syntax` block. Multiple blocks are
alternatives for the same construct; a parsed node records the zero-based
alternative index in `variant`. A block may also have an optional stable name,
available from `ConstructSpec::variant_name(index)`:

```langmeld
construct "function" {
    syntax "parenthesized" {
        literal "fn";
        identifier "name";
        literal "(";
        optional "parameters" {
            identifier "parameter";
        }
        literal ")";
        literal "->";
        identifier "return_type";
    }
    syntax "legacy" {
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
- `choice "field" { value "..."; ... }` captures one token from a finite,
  explicitly listed set. For example:

  ```langmeld
  choice "direction" {
      value "left";
      value "right";
  }
  ```

  The parser rejects undeclared values and completion suggests the declared
  values while the field is being edited. The captured value remains token
  text; hosts decide what it means. TextMate and Tree-sitter exports include
  the alternatives when their lexical models can represent them. Suggestions
  use the `enum_member` editor category by default. Override it in the choice
  declaration when the values fit another category:

  ```langmeld
  choice "mode" {
      completion keyword;
      value "automatic";
      value "manual";
  }
  ```
- `expression "field" until ";";` captures the source span up to a top-level
  terminator and consumes that terminator. Nested `()`, `[]`, and `{}` groups
  may contain the same token without ending the capture.
- `optional { ... }` matches its sequence zero or one times.
- `zero_or_more { ... }` and `one_or_more { ... }` repeat a sequence; fields
  from each match are captured in source order.
- Optional and repeated groups may have a stable name, for example
  `zero_or_more "parameters" { ... }`. Names are exposed on
  `CaptureGroup::name`; they distinguish structural groups but do not assign
  language semantics. Unnamed groups remain valid.
- `syntax "name" { ... }` names a syntax alternative. Names must be unique
  within that construct; unnamed alternatives remain valid and continue to be
  identified by their zero-based `variant` index.

`ParsedConstruct::fields` remains a flat, source-ordered list for simple field
lookup. `ParsedConstruct::capture_groups` preserves which captures belonged to
each optional or repeated match, including nested groups. Each occurrence can
return its matching field slice with `captures(construct)`. This lets a host
keep related captures together, such as a parameter name and type, without
reconstructing their pairing from separate field-name lists. It preserves
grammar grouping and optional group names only; language-specific meaning
remains host-owned. The optional `langmeld-build` crate uses named fields,
groups, and variants to generate typed Rust accessors. See
[typed Rust bindings](typed-bindings.md).

If a partially matched construct fails where a complete construct allowed in
the current scope begins, the parser reports the missing syntax and resumes at
that construct. For example, a missing statement terminator does not consume
the next statement's first token.

## Completion kinds

Construct-start suggestions have no assumed semantic kind because a leading
literal may be a keyword, operator, or punctuation. Declare a kind on the
construct when the distinction is known:

```langmeld
construct "print" {
    completion keyword;
    syntax {
        literal "print";
        identifier "value";
        literal ";";
    }
}
```

This metadata affects editor presentation only; it does not change parsing.
Write the kind as an unquoted name to receive suggestions while editing the
definition. Quoted names such as `completion "keyword";` remain accepted.
Finite-choice values default to the `enum_member` category and can declare a
different kind inside the `choice` block. Rust-authored grammars can use
`PatternPart::choice_with_completion_kind`. Supported kinds are `text`,
`method`, `function`, `constructor`, `field`, `variable`,
`class`, `interface`, `module`, `property`, `unit`, `value`, `enum`, `keyword`,
`snippet`, `color`, `file`, `reference`, `folder`, `enum_member`, `constant`,
`struct`, `event`, `operator`, and `type_parameter`.

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

The Langmeld definition helpers, `decode_definition_string` and
`quote_definition_string`, decode and encode quoted strings in
.langmeld definition files. They do not define string escaping for languages
built with Langmeld.

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

Use a custom `LanguageParser` when syntax needs rules that token patterns
cannot express. The [Brainfuck example](../../examples/brainfuck) uses one
because `[` is both a command and the opening delimiter of its loop.

## Blocks

A construct may declare one delimited block. The delimiters are exact lexer
tokens consumed after the construct's syntax pattern. `allow` lists construct
names permitted as direct children. When several blocks share the same child
group, constructs can implement a named category and a block can allow that
category:

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

Because the opening delimiter follows the construct pattern, this block form
does not model syntax where the opening token is itself the entire construct
prefix, such as Brainfuck's `[...]` loop. Use a custom parser for that syntax;
do not add a placeholder keyword to the user's language.

## Parser resource limits

The generic parser defaults to a 4 MiB source limit, 250,000 lexer tokens,
250,000 parsed constructs, 128 nested blocks, and 1,000,000 pattern-matching
steps. `ParseResult::resource_limited()` indicates that one of these bounds
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
valid prefix. Custom lexer diagnostics must refer to the exact source snapshot
and use valid UTF-8 ranges; malformed findings are suppressed and reported as
`LM-LEX-004`. These limits do not constrain custom `LanguageParser`
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

`ExpressionParseResult::resource_limited()` distinguishes a truncated token stream
or depth-limited parse. The expression parser caps configured expression-tree
depth at 128, including long member or operator chains; host parsers that do not
use it own their resource policy.

## Loading a project

`parse_language_definition` handles one source file and reports an error if
that file contains imports. For a normal manifest-backed filesystem project,
`langmeld-project` loads the manifest, resolves imports, and returns the parsed
definition together with all source snapshots:

```rust,ignore
use langmeld_core::format_diagnostics;
use langmeld_project::{LoadedDefinitionProjectParts, load_definition_project};

let project = load_definition_project(".")?;
let LoadedDefinitionProjectParts {
    definition, sources, ..
} = project.into_parts();
if !definition.is_valid() {
    eprintln!("{}", format_diagnostics(&sources, &definition.diagnostics));
}
let language = definition.language;
```

Definition syntax and validation errors stay in the returned diagnostics so
the caller can render them with the included source snapshots using the
optional plain-text `format_diagnostics` helper. Borrow the loaded result with
`root()`, `manifest()`, `definition()`, and `sources()`, or consume it with
`into_parts()` when ownership is useful. File-system and manifest failures are
returned as errors. Use `load_definition_project_from_manifest` when the
manifest is not at the project root.

When a host only needs a valid grammar, `load_language_spec(".")` provides the
short path. It returns the `LanguageSpec` directly and reports invalid
definitions as `LanguageSpecLoadError`, retaining diagnostics and their source
snapshots. Use `load_definition_project` when tooling needs to inspect all
definition diagnostics, including non-fatal findings, without converting them
into a load error.

For custom source storage, embedded modules, or editor overlays, use
`InMemoryDefinitionResolver` or implement `DefinitionResolver` directly. The
in-memory resolver keeps project-relative keys and source snapshots without
filesystem access:

```rust,ignore
use langmeld_core::SourceFile;
use langmeld_language::parse_language_definition_project;
use langmeld_project::InMemoryDefinitionResolver;

let mut resolver = InMemoryDefinitionResolver::new();
resolver.insert(
    "language/main.langmeld",
    SourceFile::new("language/main.langmeld", language_source),
)?;
resolver.insert(
    "language/constructs/statements.langmeld",
    SourceFile::new("language/constructs/statements.langmeld", statements_source),
)?;
let root = resolver.load("language/main.langmeld")?;
let result = parse_language_definition_project(root, &resolver);
let sources = resolver.sources();
```

To keep the project root and local constructs on disk while adding crate-bundled
definitions, use `OverlayDefinitionResolver`. It checks the in-memory sources
first for each import and falls back to the project directory for other paths:

```rust,ignore
use langmeld_core::SourceFile;
use langmeld_language::parse_language_definition_project;
use langmeld_project::{InMemoryDefinitionResolver, OverlayDefinitionResolver};

let mut bundled = InMemoryDefinitionResolver::new();
bundled.insert(
    "language/extensions/geometry/drawing.langmeld",
    SourceFile::new("language/extensions/geometry/drawing.langmeld", drawing_source),
)?;
let resolver = OverlayDefinitionResolver::new(".", bundled)?;
let root = resolver.load("language/main.langmeld")?;
let result = parse_language_definition_project(root, &resolver);
let sources = resolver.sources();
```

When an in-memory key matches a file path, the in-memory source takes
precedence. This makes a language project's composition explicit and lets a
host control which third-party definition crates it registers.

The parser remains independent of filesystem access. Hosts can implement
`DefinitionResolver` for other storage models; keys identify logical sources
for de-duplication and cycle detection. Import cycles are errors, repeated
imports of an already loaded key are ignored, and import nesting is bounded at
64 sources. Diagnostics retain the source name and span that produced each
error. The built-in resolvers use `/`-separated project-relative imports and
reject absolute paths, paths that escape the project root, and components that
are invalid on Windows: reserved characters or device names, control
characters, and names ending in a dot or space. This policy applies on every
host so a project can move between operating systems. Directory-backed imports
also reject symlink escapes.

## Defining a grammar entirely in Rust

`.langmeld` is optional. Build the same validated grammar model directly with
`LanguageSpec`, `ConstructSpec`, and `PatternPart`, then organize reusable
constructs in ordinary Rust modules:

```rust,ignore
use langmeld_language::{CompletionKind, ConstructSpec, LanguageSpec, PatternPart};

let print = ConstructSpec::new(
    "print",
    [
        PatternPart::literal("print"),
        PatternPart::string("value"),
        PatternPart::literal(";"),
    ],
)
.implements(&["statement"])
.with_completion_kind(CompletionKind::Keyword);

let program = ConstructSpec::new("program", [PatternPart::literal("program")])
    .block_with_categories("{", "}", &[], &["statement"]);

let grammar = LanguageSpec::builder("Tiny")
    .construct(program)
    .construct(print)
    .with_root_constructs(&["program"])
    .build()?;
```

This grammar accepts `program { print "Hello"; }`. It defines syntax and
completion metadata only; implement execution or other language behavior in
Rust handlers or other host components. Use `LanguageSpec::into_builder()` to
extend a loaded `.langmeld` grammar, or `LanguageSpecBuilder::extend()` to add
constructs collected from Rust modules.

## Mixing `.langmeld` and Rust-authored constructs

Load a declarative project as a `LanguageSpec`, then use `into_builder()` to
add constructs declared in Rust. This keeps one validated grammar model while
allowing a language to split its syntax declarations between files and Rust
modules:

```rust,ignore
use langmeld_language::{ConstructSpec, PatternPart};
use langmeld_project::load_language_spec;

let spec = load_language_spec(".")?;
let language = spec
    .into_builder()
    .construct(ConstructSpec::new(
        "version",
        [
            PatternPart::literal("version"),
            PatternPart::integer("number"),
            PatternPart::literal(";"),
        ],
    ))
    .build()?;
```

The builder validates the full construct set at `build()`, so block rules and
category membership can refer to constructs added from Rust. It preserves the
loaded lexical rules. When the definition has explicit `root` declarations,
the builder preserves that allowlist; add a Rust construct to the root with
`with_root_constructs`. Without explicit root declarations, all constructs are
allowed at the root by default.

This adds grammar only. Register each construct's behavior separately with a
`ConstructHandler` when the language needs runtime behavior.

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
offset. The host filters candidates to those valid at that position: inspect
the parse tree for syntax context and semantic analysis for rules such as
visible names, expected types, and member access. `complete_with` checks that
the parse and analysis belong to the source revision and filters replacement
spans outside the source or inside a UTF-8 character; it cannot infer
contextual validity from a host-owned tree. Standalone `SourceFile`s also have
unique snapshot identities, so their parse and analysis results are checked
against the exact source. Use `type Analysis = ()` when completion does not need
semantic analysis. `CompletionCandidate` inserts plain text by default. Use
`with_insert_text_format(CompletionInsertTextFormat::Snippet)` for editor
snippet syntax; the LSP adapter emits those items only when the client advertises
snippet support. `CompletionKind::Snippet` controls the item's displayed
category and is independent of its insertion format. The
[Schema example](../../examples/schema-language/README.md) shows a completer
that first identifies a type-reference position, then offers only names from
semantic analysis.

For definition and references queries, implement `LanguageNavigator<P>`. The
host receives the same parsed and optional analyzed snapshot and returns
`SourceLocation`s for the selected symbol. This supports different resolution
policies while keeping the result consumable by editor adapters. `definition_at`
and `references_at` reject stale snapshots, invalid query offsets, malformed
spans, and invalid locations in the current source. Hosts remain responsible
for validating locations in other source files.

## Syntax completions

`complete_at` returns grammar-derived construct-start tokens accepted at a byte
offset. Suggestions follow the current root or nested block scope, filter by a
partially typed identifier, include the active block's closing delimiter, and
provide a replacement range. Use `complete_at_with_lexer` with a custom lexer.
These completions come from declared syntax only; field values and semantic
suggestions require host-provided analysis. Custom parsers do not receive these
grammar-derived suggestions.

The built-in definition LSP also completes construct names in `root` and
`allow` references, and category names in `implements` and `allow_category`
references. With a project manifest, these suggestions come from the composed
definition, including imported files. It suggests project-local `.langmeld`
files and folders inside `import` paths. Reference candidates come from the
current root and imported source snapshots, including declarations the parser
can recognize while other edits are incomplete. Open buffers override disk
snapshots.
