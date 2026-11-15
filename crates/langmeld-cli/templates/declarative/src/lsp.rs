use std::error::Error;

use langmeld_lsp::{DeclarativeBackend, LanguageServerInfo, run_stdio};

/// Starts the project language server over standard input and output.
///
/// # Errors
///
/// Returns an error if the language definition cannot be loaded or the LSP
/// connection cannot be served.
pub fn run_stdio_server() -> Result<(), Box<dyn Error + Send + Sync>> {
    let language = crate::load()?;
    let server_info =
        LanguageServerInfo::new(env!("CARGO_PKG_NAME")).with_version(env!("CARGO_PKG_VERSION"));
    let backend =
        DeclarativeBackend::new(language.specification().clone()).with_server_info(server_info);

    run_stdio(backend)
}
