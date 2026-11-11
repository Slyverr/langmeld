use std::collections::HashMap;

#[derive(Default)]
pub(super) struct RuntimeState {
    pub(super) variables: HashMap<String, String>,
    pub(super) output: String,
}

pub(super) fn decode_string(literal: &str) -> Result<String, String> {
    let body = literal
        .strip_prefix('"')
        .and_then(|literal| literal.strip_suffix('"'))
        .ok_or_else(|| "expected a quoted string literal".to_owned())?;
    let mut characters = body.chars();
    let mut value = String::with_capacity(body.len());

    while let Some(character) = characters.next() {
        if character != '\\' {
            value.push(character);
            continue;
        }

        let escaped = characters
            .next()
            .ok_or_else(|| "string literal ends with an incomplete escape".to_owned())?;
        value.push(match escaped {
            '"' => '"',
            '\\' => '\\',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            other => return Err(format!("unsupported escape `\\{other}`")),
        });
    }

    Ok(value)
}
