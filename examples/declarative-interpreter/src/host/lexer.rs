// <langmeld:callback:lexer:custom_lexer>
/// Extend Langmeld's lexer with `langmeld::language::lex_source(source)`, or return `None` to use it unchanged.
/// Custom tokens must have ordered, non-overlapping UTF-8 byte spans. Omit the EOF sentinel.
pub fn custom_lexer() -> Option<langmeld::language::CustomLexer> {
    // <langmeld:body:lexer:custom_lexer>
    None
    // </langmeld:body:lexer:custom_lexer>
}
// </langmeld:callback:lexer:custom_lexer>
