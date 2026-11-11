# Analysis passes

Semantic rules belong to the language host. `LanguageAnalyzer` remains the
single callback option for a simple analysis; when analysis has distinct
ordered stages, `LanguagePass` and `AnalysisContext` provide a small shared
lifecycle without imposing a type system, symbol table, or execution model.

## Run ordered passes

Each pass receives the current source snapshot, parsed document, and
language-owned state. It returns diagnostics for that stage. The context runs
passes in the order selected by the host, retains all diagnostics, and returns
an `AnalysisDocument` tied to the source snapshot:

```rust,ignore
let mut analysis = AnalysisContext::new(source, parsed, LanguageFacts::default());

analysis.run_pass(&CollectDeclarations);
analysis.run_pass(&ResolveReferences);
analysis.run_pass(&CheckTypes);

let result = analysis.finish();
```

Implement a pass for the language's parsed tree and state:

```rust,ignore
impl LanguagePass<Program, LanguageFacts> for CollectDeclarations {
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

`LanguagePass` is generic over the parser's tree and host state. A custom
`LanguageParser` can use the same lifecycle as the built-in parser. Passes may
share typed facts through the host-owned state, and each pass can inspect
`parsed.diagnostics` to decide how to handle a partial tree.

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

The [Aster example](../../examples/aster-language) demonstrates a custom parser
with declaration collection followed by function-body analysis. Its types,
scope rules, and resolution policies remain specific to Aster.
