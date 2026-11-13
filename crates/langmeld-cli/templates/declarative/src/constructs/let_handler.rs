use langmeld_language::{ConstructContext, {{HANDLER_TRAIT}}, HandlerControl};

use crate::runtime::RuntimeState;

const STRING_CODE: &str = "{{DIAGNOSTIC_PREFIX}}-001";

pub(super) struct LetHandler;

impl {{HANDLER_TRAIT}}<RuntimeState> for LetHandler {
    {{CONSTRUCT_DECLARATION}}

    {{HANDLE_SIGNATURE}} {
        {{CAPTURE_ACCESS}}
        let value = decode_string(value.text)
            .map_err(|message| context.error_at(STRING_CODE, message, value.span))?;

        context.state_mut().variables.insert(name, value);

        Ok(HandlerControl::Continue)
    }
}

fn decode_string(literal: &str) -> Result<String, String> {
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
