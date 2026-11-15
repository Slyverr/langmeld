use std::error::Error;

use langmeld_lsp::{DeclarativeBackend, run_stdio};

/// Starts the project language server over standard input and output.
///
/// # Errors
///
/// Returns an error if the language definition cannot be loaded or the LSP
/// connection cannot be served.
pub fn run_stdio_server() -> Result<(), Box<dyn Error + Send + Sync>> {
    let language = crate::load()?;
    let backend = DeclarativeBackend::new(language.specification().clone());

    run_stdio(backend)
}
