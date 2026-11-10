use std::error::Error;

use langmeld_lsp::{HostLanguageBackend, run_stdio};
use langmeld_schema_language_example::{
    SchemaAnalyzer, SchemaCompleter, SchemaNavigator, SchemaParser,
};

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let backend = HostLanguageBackend::new(SchemaParser)
        .with_analyzer(SchemaAnalyzer)
        .with_completer(SchemaCompleter)
        .with_navigator(SchemaNavigator);
    run_stdio(backend)
}
