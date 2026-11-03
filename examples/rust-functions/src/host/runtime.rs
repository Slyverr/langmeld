use langmeld::core::ValidatedLanguageSpec;
use langmeld::language::{ConfiguredLanguageParser, PreparedLanguage};

/// Binds the callbacks generated for this language to its validated model.
pub fn configured_parser(spec: ValidatedLanguageSpec) -> Result<ConfiguredLanguageParser, String> {
    let prepared = PreparedLanguage::new(spec).map_err(|error| error.to_string())?;
    let parser = prepared
        .with_custom_parsers(crate::host::parser::custom_parser_callbacks())
        .map_err(|error| error.to_string())?;
    Ok(match crate::host::lexer::custom_lexer() {
        Some(lexer) => parser.with_custom_lexer(lexer),
        None => parser,
    })
}
