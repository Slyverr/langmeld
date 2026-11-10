//! Language Server Protocol transport and adapters for Langmeld languages.
#![deny(missing_docs)]

mod backend;
mod conversion;
mod document_symbol;
mod host_backend;
mod hover;
mod semantic;
mod server;

pub use backend::{
    BackendCapabilities, BackendDocument, BackendSource, BackendWorkspace,
    BackendWorkspaceCapabilities, DeclarativeBackend, LanguageBackend, LanguageDefinitionBackend,
};
pub use document_symbol::{DocumentSymbolInfo, DocumentSymbolKind};
pub use host_backend::{HostDocument, HostLanguageBackend, NoAnalyzer};
pub use hover::{HoverContent, HoverInfo};
pub use semantic::{SemanticToken, SemanticTokenLegend};
pub use server::{run_connection, run_stdio};
