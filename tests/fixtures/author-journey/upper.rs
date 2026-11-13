use langmeld_language::{ConstructContext, HandlerControl, TypedConstructHandler};

use crate::runtime::RuntimeState;

const UNKNOWN_VARIABLE_CODE: &str = "DEMO-LANGUAGE-006";

pub(super) struct UpperHandler;

impl TypedConstructHandler<RuntimeState> for UpperHandler {
    type View<'source> = crate::generated::constructs::Upper<'source>;

    fn handle<'source>(
        &self,
        context: &mut ConstructContext<'source, RuntimeState>,
        syntax: Self::View<'source>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic>
    where
        RuntimeState: 'source,
        (): 'source,
    {
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
