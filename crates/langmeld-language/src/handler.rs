//! Construct-level behavior dispatch for hosts that use parsed constructs.

// Returning the shared diagnostic type directly keeps handler errors ergonomic
// and lets authors use `?` without wrapping or unwrapping diagnostics.
#![allow(clippy::result_large_err)]

use std::{collections::HashMap, error::Error, fmt};

use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::{CapturedField, CapturedValue, ParsedConstruct};

/// Behavior associated with one stable construct name from a language definition.
///
/// The handler's state is entirely host-defined. A handler can inspect captured
/// fields, update that state, and explicitly dispatch child constructs when its
/// semantics require it. Langmeld does not assume that child constructs should
/// always be visited or executed.
pub trait ConstructHandler<State, Signal = ()> {
    /// Name of the construct declared in `.langmeld`.
    const CONSTRUCT: &'static str;

    /// Handles one parsed construct.
    fn handle(
        &self,
        construct: &ParsedConstruct,
        context: &mut ConstructContext<'_, State, Signal>,
    ) -> Result<HandlerControl<Signal>, Diagnostic>;
}

/// Whether dispatch should continue or propagate a host-defined signal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandlerControl<Signal = ()> {
    /// Continue with the next construct in the current block.
    Continue,
    /// Stop this block and return the host-defined signal to its caller.
    Signal(Signal),
}

trait ErasedConstructHandler<State, Signal> {
    fn handle(
        &self,
        construct: &ParsedConstruct,
        context: &mut ConstructContext<'_, State, Signal>,
    ) -> Result<HandlerControl<Signal>, Diagnostic>;
}

struct HandlerAdapter<Handler>(Handler);

impl<State, Signal, Handler> ErasedConstructHandler<State, Signal> for HandlerAdapter<Handler>
where
    Handler: ConstructHandler<State, Signal>,
{
    fn handle(
        &self,
        construct: &ParsedConstruct,
        context: &mut ConstructContext<'_, State, Signal>,
    ) -> Result<HandlerControl<Signal>, Diagnostic> {
        self.0.handle(construct, context)
    }
}

/// Registration failure for a construct handler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandlerRegistrationError {
    /// A handler declared an empty construct name.
    EmptyConstructName,
    /// Another handler is already registered for this construct.
    DuplicateConstruct(String),
}

impl fmt::Display for HandlerRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyConstructName => {
                formatter.write_str("construct handler name cannot be empty")
            }
            Self::DuplicateConstruct(name) => {
                write!(formatter, "a handler is already registered for `{name}`")
            }
        }
    }
}

impl Error for HandlerRegistrationError {}

/// Registered behavior for parsed constructs.
///
/// Registration is explicit and deterministic. Dispatch visits only the
/// children of a block; each handler decides whether and when nested children
/// should be dispatched.
pub struct ConstructHandlerRegistry<State, Signal = ()> {
    handlers: HashMap<&'static str, Box<dyn ErasedConstructHandler<State, Signal>>>,
}

impl<State, Signal> Default for ConstructHandlerRegistry<State, Signal> {
    fn default() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }
}

impl<State, Signal> ConstructHandlerRegistry<State, Signal> {
    /// Creates an empty handler registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one handler, rejecting empty or duplicate construct names.
    pub fn register<Handler>(&mut self, handler: Handler) -> Result<(), HandlerRegistrationError>
    where
        Handler: ConstructHandler<State, Signal> + 'static,
    {
        let name = Handler::CONSTRUCT;
        if name.is_empty() {
            return Err(HandlerRegistrationError::EmptyConstructName);
        }
        if self.handlers.contains_key(name) {
            return Err(HandlerRegistrationError::DuplicateConstruct(
                name.to_owned(),
            ));
        }

        self.handlers
            .insert(name, Box::new(HandlerAdapter(handler)));

        Ok(())
    }

    /// Dispatches every direct child of `block` in source order.
    ///
    /// The block itself is not dispatched. This lets a language use its root
    /// construct as a structural container without requiring a user handler.
    pub fn dispatch_block(
        &self,
        source: &SourceFile,
        block: &ParsedConstruct,
        state: &mut State,
    ) -> Result<HandlerControl<Signal>, Diagnostic> {
        for construct in &block.children {
            match self.dispatch_one(source, construct, state)? {
                HandlerControl::Continue => {}
                signal @ HandlerControl::Signal(_) => return Ok(signal),
            }
        }

        Ok(HandlerControl::Continue)
    }

    fn dispatch_one(
        &self,
        source: &SourceFile,
        construct: &ParsedConstruct,
        state: &mut State,
    ) -> Result<HandlerControl<Signal>, Diagnostic> {
        let Some(handler) = self.handlers.get(construct.name.as_str()) else {
            return Err(Diagnostic::error(
                "LM-HANDLER-001",
                format!(
                    "no handler is registered for construct `{}`",
                    construct.name
                ),
                source,
                construct.span,
            ));
        };

        let mut context = ConstructContext {
            source,
            state,
            handlers: self,
        };

        handler.handle(construct, &mut context)
    }
}

/// Source-aware access to host state and nested construct dispatch.
pub struct ConstructContext<'a, State, Signal = ()> {
    source: &'a SourceFile,
    state: &'a mut State,
    handlers: &'a ConstructHandlerRegistry<State, Signal>,
}

impl<'source, State, Signal> ConstructContext<'source, State, Signal> {
    /// Returns the immutable source snapshot containing the current construct.
    #[must_use]
    pub fn source(&self) -> &SourceFile {
        self.source
    }

    /// Returns the host-defined state.
    #[must_use]
    pub fn state(&self) -> &State {
        self.state
    }

    /// Returns mutable access to the host-defined state.
    pub fn state_mut(&mut self) -> &mut State {
        self.state
    }

    /// Finds a named captured field or returns a source-located diagnostic.
    pub fn field<'node>(
        &self,
        construct: &'node ParsedConstruct,
        name: &str,
    ) -> Result<&'node CapturedField, Diagnostic> {
        construct
            .fields
            .iter()
            .find(|field| field.name == name)
            .ok_or_else(|| {
                Diagnostic::error(
                    "LM-HANDLER-002",
                    format!(
                        "construct `{}` has no captured field `{name}`",
                        construct.name
                    ),
                    self.source,
                    construct.span,
                )
            })
    }

    /// Returns the exact source text captured by a named field.
    pub fn field_text(
        &self,
        construct: &ParsedConstruct,
        name: &str,
    ) -> Result<&'source str, Diagnostic> {
        let field = self.field(construct, name)?;

        self.source
            .text
            .get(field.span.start..field.span.end)
            .ok_or_else(|| {
                Diagnostic::error(
                    "LM-HANDLER-003",
                    format!("captured field `{name}` has an invalid source span"),
                    self.source,
                    field.span,
                )
            })
    }

    /// Returns an identifier capture or a source-located kind diagnostic.
    pub fn field_identifier<'node>(
        &self,
        construct: &'node ParsedConstruct,
        name: &str,
    ) -> Result<&'node str, Diagnostic> {
        let field = self.field(construct, name)?;

        match &field.value {
            CapturedValue::Identifier(value) => Ok(value),
            _ => Err(Diagnostic::error(
                "LM-HANDLER-004",
                format!("captured field `{name}` is not an identifier"),
                self.source,
                field.span,
            )),
        }
    }

    /// Dispatches a construct's direct children in source order.
    ///
    /// Handlers call this only when the language's semantics require child
    /// processing. For example, a conditional handler can dispatch its body
    /// only when the condition is true.
    pub fn dispatch_children(
        &mut self,
        construct: &ParsedConstruct,
    ) -> Result<HandlerControl<Signal>, Diagnostic> {
        self.handlers
            .dispatch_block(self.source, construct, self.state)
    }

    /// Creates an error diagnostic at an explicit source range.
    #[must_use]
    pub fn error_at(
        &self,
        code: impl Into<String>,
        message: impl Into<String>,
        span: Span,
    ) -> Diagnostic {
        Diagnostic::error(code, message, self.source, span)
    }
}
