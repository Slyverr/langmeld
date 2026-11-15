fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    langmeld_declarative_functions_example::lsp::run_stdio()
}
