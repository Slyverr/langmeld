mod compute;
mod draw;
mod emit;
mod let_handler;
mod print;

use langmeld_language::{ConstructHandlerRegistry, HandlerRegistrationError};

use crate::runtime::RuntimeState;

pub(super) fn registry() -> Result<ConstructHandlerRegistry<RuntimeState>, HandlerRegistrationError>
{
    let mut handlers = ConstructHandlerRegistry::new();

    handlers.register(compute::ComputeHandler)?;
    handlers.register(draw::DrawHandler)?;
    handlers.register(emit::EmitHandler)?;
    handlers.register(let_handler::LetHandler)?;
    handlers.register(print::PrintHandler)?;

    Ok(handlers)
}
