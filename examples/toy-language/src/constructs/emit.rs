use langmeld_language::{ConstructContext, ConstructHandler, HandlerControl};

use crate::runtime::{RuntimeState, decode_string};

pub(super) struct EmitHandler;

impl ConstructHandler<RuntimeState> for EmitHandler {
    const CONSTRUCT: &'static str = "emit";

    fn handle(
        &self,
        context: &mut ConstructContext<'_, RuntimeState>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        let mut messages = Vec::new();
        for field in context.fields("message") {
            let text = context.captured_text(field)?;
            let message = decode_string(text)
                .map_err(|message| context.error_at("TOY-001", message, field.span))?;
            messages.push(message);
        }

        for message in messages {
            let output = &mut context.state_mut().output;
            output.push_str(&message);
            output.push('\n');
        }

        Ok(HandlerControl::Continue)
    }
}
