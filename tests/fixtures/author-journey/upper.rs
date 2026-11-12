use langmeld_language::{ConstructContext, ConstructHandler, HandlerControl};

use crate::runtime::RuntimeState;

const UNKNOWN_VARIABLE_CODE: &str = "DEMO-LANGUAGE-006";

pub(super) struct UpperHandler;

impl ConstructHandler<RuntimeState> for UpperHandler {
    const CONSTRUCT: &'static str = crate::generated::constructs::Upper::NAME;

    fn handle(
        &self,
        context: &mut ConstructContext<'_, RuntimeState>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        let syntax = context.typed::<crate::generated::constructs::Upper>()?;
        let name = syntax.name()?;
        let value = context
            .state()
            .variables
            .get(name.text)
            .cloned()
            .ok_or_else(|| {
                context.error_at(
                    UNKNOWN_VARIABLE_CODE,
                    format!("unknown variable `{}`", name.text),
                    name.span,
                )
            })?;

        let value = value.to_uppercase();
        let state = context.state_mut();
        state.output.push_str(&value);
        state.output.push('\n');

        Ok(HandlerControl::Continue)
    }
}
