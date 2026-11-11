// Langmeld manages declarations inside these markers when generating handlers.
// <langmeld:begin modules>
mod let_handler;
mod print;
// <langmeld:end modules>

use langmeld_language::{ConstructHandlerRegistry, HandlerRegistrationError};

use crate::runtime::RuntimeState;

pub(super) fn registry() -> Result<ConstructHandlerRegistry<RuntimeState>, HandlerRegistrationError>
{
    let mut handlers = ConstructHandlerRegistry::new();

    // <langmeld:begin registrations>
    handlers.register(let_handler::LetHandler)?;
    handlers.register(print::PrintHandler)?;
    // <langmeld:end registrations>

    Ok(handlers)
}
