use langmeld_language::{ConstructContext, ConstructHandler, HandlerControl};

use crate::runtime::RuntimeState;

pub(super) struct LetHandler;

impl ConstructHandler<RuntimeState> for LetHandler {
    const CONSTRUCT: &'static str = "let";

    fn handle(
        &self,
        context: &mut ConstructContext<'_, RuntimeState>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        let name = context.field_identifier("name")?.to_owned();
        let value_field = context.field("value")?;
        let value_span = value_field.span;
        let value_text = context.field_text("value")?;
        let value = decode_string(value_text)
            .map_err(|message| context.error_at("EXAMPLE-001", message, value_span))?;

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
