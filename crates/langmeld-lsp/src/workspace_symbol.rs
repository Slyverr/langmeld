use langmeld_core::SourceLocation;

use crate::DocumentSymbolKind;

/// A host-defined symbol returned by an LSP workspace search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSymbolInfo {
    /// Display name shown in the workspace results.
    pub name: String,
    /// LSP symbol category.
    pub kind: DocumentSymbolKind,
    /// Declaration location in an open or host-indexed source snapshot.
    pub location: SourceLocation,
    /// Optional containing type, module, or namespace name.
    pub container_name: Option<String>,
}

impl WorkspaceSymbolInfo {
    /// Creates a workspace symbol with its declaration location.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        kind: DocumentSymbolKind,
        location: SourceLocation,
    ) -> Self {
        Self {
            name: name.into(),
            kind,
            location,
            container_name: None,
        }
    }

    /// Adds a containing type, module, or namespace name.
    #[must_use]
    pub fn with_container_name(mut self, name: impl Into<String>) -> Self {
        self.container_name = Some(name.into());
        self
    }
}
