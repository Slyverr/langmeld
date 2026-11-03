// Custom parser callbacks derived from the language definition.
// Replace each TODO with a parser that returns source-spanned nodes.

// <langmeld:registry:parser>
/// Callbacks required by constructs using `custom_parser`.
pub fn custom_parser_callbacks() -> Vec<langmeld::language::CustomConstructParser> {
    vec![]
}
// </langmeld:registry:parser>
