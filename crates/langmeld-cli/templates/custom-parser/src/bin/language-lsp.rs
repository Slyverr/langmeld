use std::error::Error;

use language::lsp::run_stdio_server;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    run_stdio_server()
}
