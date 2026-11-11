use langmeld_language::{ConstructContext, ConstructHandler, HandlerControl, ParsedConstruct};

use crate::runtime::{RuntimeState, decode_string};

pub(super) struct LetHandler;

impl ConstructHandler<RuntimeState> for LetHandler {
    const CONSTRUCT: &'static str = "let";

    fn handle(
        &self,
        construct: &ParsedConstruct,
        context: &mut ConstructContext<'_, RuntimeState>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        let name = context.field_identifier(construct, "name")?.to_owned();
        let value_field = context.field(construct, "value")?;
        let value_span = value_field.span;
        let value_text = context.field_text(construct, "value")?;
        let value = decode_string(value_text)
            .map_err(|message| context.error_at("TOY-001", message, value_span))?;

        context.state_mut().variables.insert(name, value);

        Ok(HandlerControl::Continue)
    }
}
