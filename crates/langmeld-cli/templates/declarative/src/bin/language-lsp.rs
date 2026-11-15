use std::error::Error;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    language::lsp::run_stdio_server()
}
