// Custom parser callbacks derived from the language definition.
// Replace each TODO with a parser that returns source-spanned nodes.

// <langmeld:registry:parser>
/// Callbacks required by constructs using `custom_parser`.
pub fn custom_parser_callbacks() -> Vec<langmeld::language::CustomConstructParser> {
    vec![]
}
// </langmeld:registry:parser>

// <langmeld:registry:expression_atom_parser>
/// Callbacks required by expression grammars using host-defined atoms.
pub fn expression_atom_parser_callbacks()
-> Vec<langmeld::language::ExpressionAtomParserRegistration> {
    vec![langmeld::language::ExpressionAtomParserRegistration::new(
        "aster_prefixed_number",
        |source: &langmeld::core::SourceFile, range: langmeld::core::Span, offset: usize| {
            let Some(fragment) = source.text.get(offset..range.end) else {
                return Err(langmeld::core::Diagnostic::error(
                    "ASTER1010",
                    "expression atom offset is outside the source range",
                    langmeld::core::Span::new(offset, offset),
                ));
            };
            let Some(digits) = fragment.strip_prefix('@') else {
                return Ok(None);
            };
            let length = digits
                .char_indices()
                .take_while(|(_, character)| character.is_ascii_digit())
                .map(|(index, character)| index + character.len_utf8())
                .last()
                .unwrap_or(0);
            if length == 0 {
                return Ok(None);
            }
            let end = offset + 1 + length;
            Ok(Some(langmeld::expression::CustomAtomToken {
                kind: "aster_number".to_owned(),
                text: source.text[offset..end].to_owned(),
                span: langmeld::core::Span::new(offset, end),
            }))
        },
    )]
}
// </langmeld:registry:expression_atom_parser>
