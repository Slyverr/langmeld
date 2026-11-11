use langmeld_language::{ConstructContext, ConstructHandler, HandlerControl};

use crate::runtime::{RuntimeState, decode_string};

pub(super) struct DrawHandler;

impl ConstructHandler<RuntimeState> for DrawHandler {
    const CONSTRUCT: &'static str = "draw";

    fn handle(
        &self,
        context: &mut ConstructContext<'_, RuntimeState>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        let shape = context.field_text("shape")?;
        let label = match context.fields("label").next() {
            Some(field) => {
                let text = context.captured_text(field)?;
                Some(
                    decode_string(text)
                        .map_err(|message| context.error_at("TOY-001", message, field.span))?,
                )
            }
            None => None,
        };

        let output = &mut context.state_mut().output;
        output.push_str(&format!("Drawing {shape}"));
        if let Some(label) = label {
            output.push_str(&format!(" labeled {label:?}"));
        }
        output.push_str(".\n");

        Ok(HandlerControl::Continue)
    }
}
