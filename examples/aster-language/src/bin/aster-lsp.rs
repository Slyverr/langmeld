use langmeld_aster_language_example::AsterBackend;

fn main() {
    if let Err(error) = langmeld_lsp::run_stdio(AsterBackend) {
        eprintln!("Aster language server failed: {error}");
        std::process::exit(1);
    }
}
