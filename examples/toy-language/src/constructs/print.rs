use langmeld_language::{ConstructContext, ConstructHandler, HandlerControl, ParsedConstruct};

use crate::runtime::RuntimeState;

pub(super) struct PrintHandler;

impl ConstructHandler<RuntimeState> for PrintHandler {
    const CONSTRUCT: &'static str = "print";

    fn handle(
        &self,
        construct: &ParsedConstruct,
        context: &mut ConstructContext<'_, RuntimeState>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        let name = context.field_identifier(construct, "name")?.to_owned();
        let value_field = context.field(construct, "name")?;
        let value = context
            .state()
            .variables
            .get(&name)
            .cloned()
            .ok_or_else(|| {
                context.error_at(
                    "TOY-002",
                    format!("unknown variable `{name}`"),
                    value_field.span,
                )
            })?;

        context.state_mut().output.push_str(&value);
        context.state_mut().output.push('\n');

        Ok(HandlerControl::Continue)
    }
}
