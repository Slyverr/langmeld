use std::error::Error;

use langmeld_lsp::{HostLanguageBackend, LanguageServerInfo, run_stdio};

use crate::SayParser;

/// Starts the project language server over standard input and output.
///
/// # Errors
///
/// Returns an I/O or protocol error when the server cannot be initialized or
/// served.
pub fn run_stdio_server() -> Result<(), Box<dyn Error + Send + Sync>> {
    let server_info = LanguageServerInfo::new(env!("CARGO_PKG_NAME"))
        .with_version(env!("CARGO_PKG_VERSION"));
    let backend = HostLanguageBackend::new(SayParser).with_server_info(server_info);

    run_stdio(backend)
}
