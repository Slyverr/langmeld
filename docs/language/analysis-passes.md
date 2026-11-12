# Analysis passes

Semantic rules belong to the language host. `LanguageAnalyzer` remains the
single callback option for a simple analysis; when analysis has distinct
ordered stages, `LanguagePass` and `AnalysisContext` provide a small shared
lifecycle without imposing a type system, symbol table, or execution model.

## Run ordered passes

Each pass receives the parser's own result, the current source snapshot, and
language-owned state. It returns diagnostics for that stage. The context runs
passes in the order selected by the host, retains all diagnostics, and returns
an `AnalysisDocument` tied to the source snapshot. This works with both the
built-in parser's `ParseResult` and a custom parser's `ParsedDocument<Tree>`:

```rust,ignore
let mut analysis = AnalysisContext::new(source, parsed, LanguageFacts::default())?;

analysis.run_pass(&CollectDeclarations);
analysis.run_pass(&ResolveReferences);
analysis.run_pass(&CheckTypes);

let result = analysis.finish();
```

`AnalysisContext::new` checks source identity and returns a diagnostic if
`parsed` belongs to a different snapshot, including when both sources were
created directly with `SourceFile::new`. The context does not clone or convert
the parse result. Each pass can inspect `parsed.diagnostics()` and choose
whether it can proceed with a partial parse tree.

For a language using the declarative parser, implement a pass over
`langmeld_language::ParseResult`:

```rust,ignore
impl LanguagePass<ParseResult, LanguageFacts> for CheckNames {
    fn run(
        &self,
        source: &SourceFile,
        parsed: &ParseResult,
        facts: &mut LanguageFacts,
    ) -> Vec<Diagnostic> {
        // Inspect parsed.constructs() and return source-located findings.
        Vec::new()
    }
}
```

For a custom parser, use its `ParsedDocument<Tree>` directly:

```rust,ignore
impl LanguagePass<ParsedDocument<Program>, LanguageFacts> for CollectDeclarations {
    fn run(
        &self,
        source: &SourceFile,
        parsed: &ParsedDocument<Program>,
        facts: &mut LanguageFacts,
    ) -> Vec<Diagnostic> {
        // Add declarations and return source-located findings.
        Vec::new()
    }
}
```

`LanguagePass` is generic over the parser result and host state. Both built-in
and custom parse results implement `AnalysisInput`; implement that trait for a
different parser result type to use the same lifecycle. Passes may share typed
facts through host-owned state, and each pass can inspect parse diagnostics to
decide how to handle partial syntax.

## Diagnostics and ordering

The context appends each pass's diagnostics in execution order and continues to
later passes even if an earlier pass reported errors. A language can skip a
stage itself when its prerequisites are unavailable. Parse diagnostics remain
on `ParsedDocument`; analysis diagnostics are returned separately on
`AnalysisDocument`.

The host chooses which passes to run and in what order. For example, collecting
function signatures before checking bodies permits forward calls. Langmeld does
not automatically add name resolution, type checking, or control-flow rules.
Simple languages can keep using one `LanguageAnalyzer` implementation and omit
the pass context entirely.

## Result validity and diagnostic severity

Diagnostics distinguish errors from warnings and informational findings.
`has_errors(&diagnostics)` checks an aggregated collection; warning and info
findings do not make a result invalid. Results expose `is_valid()` so callers
do not have to duplicate that rule. Generic and expression parse results also
report invalidity when resource limits stopped parsing, and expression results
require a parsed expression.

Invalid results still retain their partial syntax or analysis data and all
diagnostics. Hosts can use the partial data for editor features while deciding
whether to stop a later stage. When displaying findings, format the complete
diagnostic list so warnings and information are not lost:

```rust,ignore
let diagnostics = parsed
    .diagnostics()
    .iter()
    .chain(analysis.diagnostics())
    .cloned()
    .collect::<Vec<_>>();

if has_errors(&diagnostics) {
    // Report the findings and stop a pipeline that requires valid input.
}
```

The [Aster example](../../examples/aster-language) demonstrates a custom parser
with declaration collection followed by function-body analysis. Its types,
scope rules, and resolution policies remain specific to Aster.
