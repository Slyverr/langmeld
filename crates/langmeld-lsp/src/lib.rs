//! Language Server Protocol transport and adapters for Langmeld languages.
#![deny(missing_docs)]

mod backend;
mod conversion;
mod server;

pub use backend::{BackendCapabilities, BackendDocument, DeclarativeBackend, LanguageBackend};
pub use server::{run_connection, run_stdio};
