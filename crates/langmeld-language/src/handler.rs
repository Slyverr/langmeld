//! Construct-level behavior dispatch for hosts that use parsed constructs.

// Returning the shared diagnostic type directly keeps handler errors ergonomic
// and lets authors use `?` without wrapping or unwrapping diagnostics.
#![allow(clippy::result_large_err)]

use std::{collections::HashMap, error::Error, fmt};

use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::{ParseResult, ParsedConstruct, TypedConstruct};

mod context;

pub use context::{CapturedIdentifier, CapturedText, ConstructContext};

/// Behavior associated with one stable construct name from a language definition.
///
/// The handler's state is entirely host-defined. A handler can inspect captured
/// fields, update that state, and explicitly dispatch child constructs when its
/// semantics require it. Langmeld does not assume that child constructs should
/// always be visited or executed.
pub trait ConstructHandler<State, Signal = ()> {
    /// Name of the construct declared in the language specification.
    const CONSTRUCT: &'static str;

    /// Handles one parsed construct.
    fn handle(
        &self,
        context: &mut ConstructContext<'_, State, Signal>,
    ) -> Result<HandlerControl<Signal>, Diagnostic>;
}

/// Behavior for one construct using a typed view.
///
/// The registry derives the dispatch name from the view's `CONSTRUCT` value
/// and creates the view before calling the handler. This keeps typed capture
/// access and handler registration tied to one construct type. Use
/// [`ConstructHandler`] when the language is dynamic or the handler needs
/// runtime field access.
pub trait TypedConstructHandler<State, Signal = ()> {
    /// Typed projection for the current construct and source lifetime.
    type View<'source>: TypedConstruct<'source>
    where
        State: 'source,
        Signal: 'source;

    /// Handles one parsed construct after creating its typed view.
    fn handle<'source>(
        &self,
        context: &mut ConstructContext<'source, State, Signal>,
        syntax: Self::View<'source>,
    ) -> Result<HandlerControl<Signal>, Diagnostic>
    where
        State: 'source,
        Signal: 'source;
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
    fn handle<'source>(
        &self,
        context: &mut ConstructContext<'source, State, Signal>,
    ) -> Result<HandlerControl<Signal>, Diagnostic>
    where
        State: 'source,
        Signal: 'source;
}

struct HandlerAdapter<Handler>(Handler);

impl<State, Signal, Handler> ErasedConstructHandler<State, Signal> for HandlerAdapter<Handler>
where
    Handler: ConstructHandler<State, Signal>,
{
    fn handle<'source>(
        &self,
        context: &mut ConstructContext<'source, State, Signal>,
    ) -> Result<HandlerControl<Signal>, Diagnostic>
    where
        State: 'source,
        Signal: 'source,
    {
        self.0.handle(context)
    }
}

struct TypedHandlerAdapter<Handler>(Handler);

impl<State, Signal, Handler> ErasedConstructHandler<State, Signal> for TypedHandlerAdapter<Handler>
where
    Handler: TypedConstructHandler<State, Signal>,
{
    fn handle<'source>(
        &self,
        context: &mut ConstructContext<'source, State, Signal>,
    ) -> Result<HandlerControl<Signal>, Diagnostic>
    where
        State: 'source,
        Signal: 'source,
    {
        let syntax = context.typed::<Handler::View<'source>>()?;

        self.0.handle(context, syntax)
    }
}

/// Registration failure for a construct handler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandlerRegistrationError {
    /// A handler's typed view or declaration has an empty construct name.
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
/// Registration is explicit and deterministic. Each dispatch entry point
/// states whether it visits a construct or its children; handlers decide
/// whether and when nested children should be dispatched.
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
        self.register_erased(Handler::CONSTRUCT, Box::new(HandlerAdapter(handler)))
    }

    /// Registers a typed handler under the construct name from its view.
    ///
    /// The view is created for each dispatched construct before the handler
    /// runs. If a runtime-loaded definition does not match the view's expected
    /// capture shape, dispatch returns its source-located diagnostic.
    pub fn register_typed<Handler>(
        &mut self,
        handler: Handler,
    ) -> Result<(), HandlerRegistrationError>
    where
        Handler: TypedConstructHandler<State, Signal> + 'static,
    {
        let name = typed_construct_name::<State, Signal, Handler>(&handler);

        self.register_erased(name, Box::new(TypedHandlerAdapter(handler)))
    }

    fn register_erased(
        &mut self,
        name: &'static str,
        handler: Box<dyn ErasedConstructHandler<State, Signal>>,
    ) -> Result<(), HandlerRegistrationError> {
        if name.is_empty() {
            return Err(HandlerRegistrationError::EmptyConstructName);
        }
        if self.handlers.contains_key(name) {
            return Err(HandlerRegistrationError::DuplicateConstruct(
                name.to_owned(),
            ));
        }

        self.handlers.insert(name, handler);

        Ok(())
    }

    /// Dispatches every direct child of `block` in source order.
    ///
    /// The block itself is not dispatched. This lets a language use its root
    /// construct as a structural container without requiring a user handler.
    /// `block` must be borrowed from `parsed`; the parse result and selected
    /// construct are checked against `source`. Use [`Self::dispatch_construct`]
    /// separately when the block has behavior of its own.
    ///
    /// # Errors
    ///
    /// Returns a source-located diagnostic if `parsed` belongs to another
    /// source snapshot, `block` is not part of `parsed`, or a child handler
    /// returns a diagnostic.
    pub fn dispatch_block(
        &self,
        source: &SourceFile,
        parsed: &ParseResult,
        block: &ParsedConstruct,
        state: &mut State,
    ) -> Result<HandlerControl<Signal>, Diagnostic> {
        validate_parsed_construct(source, parsed, block)?;

        self.dispatch_block_children(source, parsed, block, state)
    }

    /// Dispatches every top-level construct in a parser result, in source order.
    ///
    /// Unlike [`Self::dispatch_block`], this dispatches the top-level
    /// constructs themselves. It does not visit their children automatically;
    /// each handler decides whether nested constructs should be dispatched.
    ///
    /// The result must belong to `source`; all snapshots are checked by
    /// identity. Recovered constructs are dispatched even when
    /// the result has diagnostics; check [`ParseResult::is_valid`] first when
    /// execution should require a fully valid parse.
    pub fn dispatch_parse_result(
        &self,
        source: &SourceFile,
        parsed: &ParseResult,
        state: &mut State,
    ) -> Result<HandlerControl<Signal>, Diagnostic> {
        validate_parsed_source(source, parsed)?;

        self.dispatch_constructs(source, parsed, parsed.constructs(), state)
    }

    /// Dispatches the direct children of each top-level construct in source
    /// order, without dispatching the top-level constructs themselves.
    ///
    /// This is useful when top-level constructs are structural containers such
    /// as a `program` root. The result must belong to `source`; all snapshots
    /// are checked by identity. Recovered children are dispatched
    /// even when the result has diagnostics; check [`ParseResult::is_valid`]
    /// first when execution should require a fully valid parse.
    pub fn dispatch_top_level_children(
        &self,
        source: &SourceFile,
        parsed: &ParseResult,
        state: &mut State,
    ) -> Result<HandlerControl<Signal>, Diagnostic> {
        validate_parsed_source(source, parsed)?;

        for construct in parsed.constructs() {
            match self.dispatch_block_children(source, parsed, construct, state)? {
                HandlerControl::Continue => {}
                signal @ HandlerControl::Signal(_) => return Ok(signal),
            }
        }

        Ok(HandlerControl::Continue)
    }

    fn dispatch_constructs(
        &self,
        source: &SourceFile,
        parsed: &ParseResult,
        constructs: &[ParsedConstruct],
        state: &mut State,
    ) -> Result<HandlerControl<Signal>, Diagnostic> {
        for construct in constructs {
            match self.dispatch_one_construct(source, parsed, construct, state)? {
                HandlerControl::Continue => {}
                signal @ HandlerControl::Signal(_) => return Ok(signal),
            }
        }

        Ok(HandlerControl::Continue)
    }

    /// Dispatches exactly one construct to its registered handler.
    ///
    /// This does not visit the construct's children automatically. The handler
    /// can call [`ConstructContext::dispatch_children`] when its semantics
    /// require nested processing. Use this method when a root or module
    /// construct has behavior of its own. `construct` must be borrowed from
    /// `parsed`; the parse result and selected construct are checked against
    /// `source`.
    ///
    /// # Errors
    ///
    /// Returns a source-located diagnostic if `parsed` belongs to another
    /// source snapshot, `construct` is not part of `parsed`, or no handler is
    /// registered for its name.
    pub fn dispatch_construct(
        &self,
        source: &SourceFile,
        parsed: &ParseResult,
        construct: &ParsedConstruct,
        state: &mut State,
    ) -> Result<HandlerControl<Signal>, Diagnostic> {
        validate_parsed_construct(source, parsed, construct)?;

        self.dispatch_one_construct(source, parsed, construct, state)
    }

    fn dispatch_block_children(
        &self,
        source: &SourceFile,
        parsed: &ParseResult,
        block: &ParsedConstruct,
        state: &mut State,
    ) -> Result<HandlerControl<Signal>, Diagnostic> {
        self.dispatch_constructs(source, parsed, &block.children, state)
    }

    fn dispatch_one_construct(
        &self,
        source: &SourceFile,
        parsed: &ParseResult,
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

        let mut context = ConstructContext::new(parsed, construct, source, state, self);

        handler.handle(&mut context)
    }
}

fn typed_construct_name<'source, State, Signal, Handler>(_handler: &'source Handler) -> &'static str
where
    Handler: TypedConstructHandler<State, Signal>,
    State: 'source,
    Signal: 'source,
{
    <Handler::View<'source> as TypedConstruct<'source>>::CONSTRUCT
}

fn validate_parsed_source(source: &SourceFile, parsed: &ParseResult) -> Result<(), Diagnostic> {
    if parsed.source_identity() != source.identity() {
        return Err(Diagnostic::error(
            "LM-HANDLER-005",
            "parse result belongs to a different source snapshot",
            source,
            Span::default(),
        ));
    }

    Ok(())
}

fn validate_parsed_construct(
    source: &SourceFile,
    parsed: &ParseResult,
    construct: &ParsedConstruct,
) -> Result<(), Diagnostic> {
    validate_parsed_source(source, parsed)?;

    if !parsed.contains_construct(construct) {
        return Err(Diagnostic::error(
            "LM-HANDLER-006",
            "selected construct does not belong to the supplied parse result",
            source,
            Span::default(),
        ));
    }

    Ok(())
}
