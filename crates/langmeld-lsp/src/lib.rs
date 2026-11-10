//! Language Server Protocol transport and adapters for Langmeld languages.
#![deny(missing_docs)]

mod backend;
mod conversion;
mod semantic;
mod server;

pub use backend::{BackendCapabilities, BackendDocument, DeclarativeBackend, LanguageBackend};
pub use semantic::{SemanticToken, SemanticTokenLegend};
pub use server::{run_connection, run_stdio};
