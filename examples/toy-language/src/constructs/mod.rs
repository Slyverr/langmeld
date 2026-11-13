mod compute;
mod draw;
mod emit;
mod let_handler;
mod print;

use langmeld_language::{ConstructHandlerRegistry, HandlerRegistrationError};

use crate::runtime::RuntimeState;

type HandlerRegistry = ConstructHandlerRegistry<'static, RuntimeState>;

pub(super) fn registry() -> Result<HandlerRegistry, HandlerRegistrationError> {
    let mut handlers = ConstructHandlerRegistry::new();

    handlers.register(compute::ComputeHandler)?;
    handlers.register(draw::DrawHandler)?;
    handlers.register(emit::EmitHandler)?;
    handlers.register(let_handler::LetHandler)?;
    handlers.register(print::PrintHandler)?;

    Ok(handlers)
}
