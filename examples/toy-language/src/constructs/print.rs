use langmeld_language::{ConstructContext, ConstructHandler, HandlerControl};

use crate::runtime::RuntimeState;

pub(super) struct PrintHandler;

impl ConstructHandler<RuntimeState> for PrintHandler {
    const CONSTRUCT: &'static str = "print";

    fn handle(
        &self,
        context: &mut ConstructContext<'_, RuntimeState>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        let name = context.field_identifier_capture("name")?;
        let value = context
            .state()
            .variables
            .get(name.text)
            .cloned()
            .ok_or_else(|| {
                context.error_at(
                    "TOY-002",
                    format!("unknown variable `{}`", name.text),
                    name.span,
                )
            })?;

        context.state_mut().output.push_str(&value);
        context.state_mut().output.push('\n');

        Ok(HandlerControl::Continue)
    }
}
