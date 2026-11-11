/// Decodes a quoted string literal from a Langmeld definition.
///
/// Accepted two-character escape pairs are (backslash, backslash),
/// (backslash, double quote), (backslash, n), (backslash, r), and (backslash,
/// t). An unquoted value, unescaped interior quote, incomplete escape, or
/// unsupported escape returns no value.
#[must_use]
pub fn decode_definition_string(value: &str) -> Option<String> {
    let inner = value.strip_prefix('"')?.strip_suffix('"')?;
    let mut decoded = String::with_capacity(inner.len());
    let mut characters = inner.chars();

    while let Some(character) = characters.next() {
        if character == '"' {
            return None;
        }

        if character != '\\' {
            decoded.push(character);
            continue;
        }

        match characters.next()? {
            '\\' => decoded.push('\\'),
            '"' => decoded.push('"'),
            'n' => decoded.push('\n'),
            'r' => decoded.push('\r'),
            't' => decoded.push('\t'),
            _ => return None,
        }
    }

    Some(decoded)
}

/// Encodes a value as a quoted string literal for a Langmeld definition.
///
/// Backslashes, double quotes, newlines, carriage returns, and tabs are escaped
/// so the returned literal can be decoded with decode_definition_string.
#[must_use]
pub fn quote_definition_string(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');

    for character in value.chars() {
        match character {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            character => quoted.push(character),
        }
    }

    quoted.push('"');
    quoted
}
