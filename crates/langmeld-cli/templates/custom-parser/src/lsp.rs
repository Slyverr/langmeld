use std::error::Error;

use langmeld_lsp::{HostLanguageBackend, run_stdio};

use crate::SayParser;

/// Starts the project language server over standard input and output.
///
/// # Errors
///
/// Returns an I/O or protocol error when the server cannot be initialized or
/// served.
pub fn run_stdio_server() -> Result<(), Box<dyn Error + Send + Sync>> {
    run_stdio(HostLanguageBackend::new(SayParser))
}
