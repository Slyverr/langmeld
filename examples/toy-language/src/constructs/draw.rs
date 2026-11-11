use langmeld_language::{ConstructContext, ConstructHandler, HandlerControl, ParsedConstruct};

use crate::runtime::RuntimeState;

pub(super) struct DrawHandler;

impl ConstructHandler<RuntimeState> for DrawHandler {
    const CONSTRUCT: &'static str = "draw";

    fn handle(
        &self,
        construct: &ParsedConstruct,
        context: &mut ConstructContext<'_, RuntimeState>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        let shape = context.field_text(construct, "shape")?;
        context
            .state_mut()
            .output
            .push_str(&format!("Drawing {shape}.\n"));

        Ok(HandlerControl::Continue)
    }
}
