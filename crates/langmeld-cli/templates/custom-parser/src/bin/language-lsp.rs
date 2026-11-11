use std::error::Error;

use langmeld_lsp::{HostLanguageBackend, run_stdio};
use language::SayParser;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    run_stdio(HostLanguageBackend::new(SayParser))
}
