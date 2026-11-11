# Upgrading

## v0.7.0 to the current development API

The changes below are on `develop` after the v0.7.0 tag. They are unreleased
and are not part of that tagged source. The release notes will name the target
version once its scope is finalized.

### Read source-associated data through accessors

Several framework-owned records now keep source identity and payload fields
private so they cannot be changed independently. Replace field access with the
matching accessor:

Core source types:

- `SourceFile.name` and `.text` → `name()` and `text()`.
- `SourceIdentity.database_id`, `.id`, and `.revision` →
  `database_id()`, `id()`, and `revision()`.
- `SourceLocation.source_name`, `.identity`, and `.span` →
  `source_name()`, `source_identity()`, and `span()`.

Language and parser results:

- `ParsedDocument.source`, `.tree`, and `.diagnostics` →
  `source_identity()`, `tree()`, and `diagnostics()`.
- `AnalysisDocument.source`, `.result`, and `.diagnostics` →
  `source_identity()`, `result()`, and `diagnostics()`.
- `ParseResult.source`, `.constructs`, `.syntax_tree`, `.diagnostics`, and
  `.resource_limited` → `source_identity()`, `constructs()`, `syntax_tree()`,
  `diagnostics()`, and `resource_limited()`.

Project and LSP results:

- `DefinitionSource.key` and `.source` → `key()` and `source()`.
- `LoadedDefinitionProject.root`, `.manifest`, `.definition`, and `.sources` →
  `root()`, `manifest()`, `definition()`, and `sources()`.
- `BackendDocument.source`, `.state`, and `.diagnostics` →
  `source_identity()`, `state()`, and `diagnostics()`.
- `BackendSource.uri` and `.source` → `uri()` and `source()`.
- `ExpressionParseResult.source`, `.expression`, `.consumed`, `.diagnostics`,
  and `.resource_limited` → `source_identity()`, `expression()`, `consumed()`,
  `diagnostics()`, and `resource_limited()`.

Owned extraction remains available through `into_parts()` on the types that
provide it. Each type's API docs describe its returned parts.

### Handle missing validated host results

`HostDocument::parsed()` and `HostDocument::analysis()` now return `Option`.
The host adapter discards results whose source identity does not match the
document snapshot, so callers must handle their absence:

```rust,ignore
let Some(parsed) = document.state().parsed() else {
    // The backend diagnostics explain why the parse was unavailable.
    return;
};

let tree = parsed.tree();
```

Analysis can be unavailable independently:

```rust,ignore
if let Some(analysis) = document.state().analysis() {
    inspect_facts(analysis.result());
}
```

### Handle source mismatch when creating an analysis context

`AnalysisContext::new` now returns `Result<AnalysisContext, Diagnostic>` and
rejects a parsed document from another database-managed source snapshot. In a
caller that can propagate diagnostics, use `?`:

```rust,ignore
let mut context = AnalysisContext::new(source, parsed, state)?;
```

`LanguageAnalyzer::analyze` returns an `AnalysisDocument` rather than a
`Result`, so convert a construction error into an analysis diagnostic, as the
examples do:

```rust,ignore
let mut context = match AnalysisContext::new(source, parsed, Facts::default()) {
    Ok(context) => context,
    Err(diagnostic) => {
        return AnalysisDocument::new(source, Facts::default(), vec![diagnostic]);
    }
};
```

Detached sources do not have database identities; callers remain responsible
for pairing those snapshots correctly.

### Bind construct handlers to their context

`ConstructHandler::handle` no longer receives a separate `ParsedConstruct`.
The context now identifies the active construct, so remove that parameter and
the construct argument to field access and child dispatch:

```rust,ignore
fn handle(
    &self,
    context: &mut ConstructContext<'_, RuntimeState>,
) -> Result<HandlerControl, Diagnostic> {
    let construct = context.construct();
    let name = context.field_identifier("name")?;

    context.dispatch_children()?;
    Ok(HandlerControl::Continue)
}
```

`field`, `fields`, `field_text`, and `field_identifier` now read from the
current construct. `dispatch_children()` likewise dispatches that construct's
children, preventing a handler from accidentally mixing a node with another
context.
