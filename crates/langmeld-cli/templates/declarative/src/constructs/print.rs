use langmeld_language::{ConstructContext, {{HANDLER_TRAIT}}, HandlerControl};

use crate::runtime::RuntimeState;

pub(super) struct PrintHandler;

impl {{HANDLER_TRAIT}}<RuntimeState> for PrintHandler {
    {{CONSTRUCT_DECLARATION}}

    {{HANDLE_SIGNATURE}} {
        {{CAPTURE_ACCESS}}
        let value = context
            .state()
            .variables
            .get(name.text)
            .cloned()
            .ok_or_else(|| {
                context.error_at(
                    "{{DIAGNOSTIC_PREFIX}}-002",
                    format!("unknown variable `{}`", name.text),
                    name.span,
                )
            })?;

        context.state_mut().output.push_str(&value);
        context.state_mut().output.push('\n');

        Ok(HandlerControl::Continue)
    }
}
