# Upgrading

## Compatibility status

Langmeld is pre-1.0.0. Its public Rust APIs and `.langmeld` syntax are still
evolving, and a minor 0.x release may contain breaking changes. Breaking
changes are called out in the changelog and migration guidance is provided
when users need to update their code or definitions. Patch releases are
reserved for compatible fixes.

The `format` field in `langmeld.toml` versions the project-manifest schema,
independently of the Langmeld crate version. Format `1` is currently supported;
other values are rejected. `.langmeld` files do not have a separate format
field, so their compatibility follows the Langmeld release. The CLI does not
currently provide an automatic migration command.

## Migrating from v0.8.0 to v0.9.0

The changes below are included in v0.9.0. See the [changelog](../../CHANGELOG.md)
for the complete release notes.

### Track and validate source snapshots

Standalone `SourceFile::new` snapshots now receive distinct, process-local
identities. Cloning a snapshot keeps its identity; constructing another source
with the same name and text creates a different identity. Parse, analysis,
completion, navigation, diagnostic, and backend validation use that identity for
standalone and database-managed sources alike. Callers no longer need to pair
standalone parse results manually.

### Use the renamed namespace accessor

`SourceFile::identity()` and source-result `source_identity()` accessors now
return `SourceIdentity` directly. `SourceIdentity::database_id()` is renamed to
`namespace_id()`, since a namespace can belong to a database or to one
standalone source. Use `SourceFile::is_database_managed()` when code needs to
distinguish those cases; do not use an absent identity as that check.

### Construct source-bound results through their APIs

Source-bound result types no longer implement `Default`: create them through
the parser or their source-aware constructors so every result retains the
snapshot it describes. Source identities are process-local validation tokens,
not persistent file or project identifiers.

## Migrating from v0.7.0 to v0.8.0

The changes below are included in v0.8.0. See the [changelog](../../CHANGELOG.md)
for the complete release notes.

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
