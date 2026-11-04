// <langmeld:callback:lexer:custom_lexer>
/// Extend Langmeld's built-in lexer, or return `None` to use the language's configured lexer.
/// Use `lex_source_with_lexical_spec` when adapting a declared comment policy.
/// Custom tokens must have ordered, non-overlapping UTF-8 byte spans. Omit the EOF sentinel.
pub fn custom_lexer() -> Option<langmeld::language::CustomLexer> {
    // <langmeld:body:lexer:custom_lexer>
    None
    // </langmeld:body:lexer:custom_lexer>
}
// </langmeld:callback:lexer:custom_lexer>
