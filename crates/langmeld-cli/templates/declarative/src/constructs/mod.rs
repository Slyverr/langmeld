// Langmeld manages declarations inside these markers when generating handlers.
// <langmeld:begin modules>
mod let_handler;
mod print;
// <langmeld:end modules>

use langmeld_language::{ConstructHandlerRegistry, HandlerRegistrationError};

use crate::runtime::RuntimeState;

type HandlerRegistry = ConstructHandlerRegistry<'static, RuntimeState>;

pub(super) fn registry() -> Result<HandlerRegistry, HandlerRegistrationError> {
    let mut handlers = ConstructHandlerRegistry::new();

    // <langmeld:begin registrations>
    handlers.{{REGISTRATION_METHOD}}(let_handler::LetHandler)?;
    handlers.{{REGISTRATION_METHOD}}(print::PrintHandler)?;
    // <langmeld:end registrations>

    Ok(handlers)
}
