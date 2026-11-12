use std::error::Error;

use langmeld_lsp::{HostLanguageBackend, run_stdio};
use langmeld_schema_language_example::{
    SchemaAnalyzer, SchemaCompleter, SchemaNavigator, SchemaParser, SchemaWorkspaceAnalyzer,
};

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let backend = HostLanguageBackend::new_with_analyzer(SchemaParser, SchemaAnalyzer)
        .with_workspace_analyzer(SchemaWorkspaceAnalyzer)
        .with_completer(SchemaCompleter)
        .with_navigator(SchemaNavigator);
    run_stdio(backend)
}
