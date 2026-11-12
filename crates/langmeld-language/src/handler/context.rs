use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::{CapturedField, CapturedValue, ParseResult, ParsedConstruct, TypedConstruct};

use super::{ConstructHandlerRegistry, HandlerControl};

/// Source-aware access to the current construct, host state, and nested dispatch.
pub struct ConstructContext<'a, State, Signal = ()> {
    parsed: &'a ParseResult,
    construct: &'a ParsedConstruct,
    source: &'a SourceFile,
    state: &'a mut State,
    handlers: &'a ConstructHandlerRegistry<State, Signal>,
}

/// An identifier captured from source, paired with its exact byte range.
///
/// This is lexical information only. It does not indicate whether the
/// identifier declares or references a language-level symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapturedIdentifier<'source> {
    /// The identifier's spelling in the source.
    pub text: &'source str,
    /// The identifier's byte range in the source snapshot.
    pub span: Span,
}

/// Exact source text captured by a construct, paired with its byte range.
///
/// This preserves the source spelling. For example, a captured string literal
/// still includes its quotes and escape sequences.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapturedText<'source> {
    /// Exact source text covered by the capture.
    pub text: &'source str,
    /// The capture's byte range in the source snapshot.
    pub span: Span,
}

impl<'source, State, Signal> ConstructContext<'source, State, Signal> {
    pub(super) fn new(
        parsed: &'source ParseResult,
        construct: &'source ParsedConstruct,
        source: &'source SourceFile,
        state: &'source mut State,
        handlers: &'source ConstructHandlerRegistry<State, Signal>,
    ) -> Self {
        Self {
            parsed,
            construct,
            source,
            state,
            handlers,
        }
    }

    /// Returns the construct currently being handled.
    #[must_use]
    pub fn construct(&self) -> &'source ParsedConstruct {
        self.construct
    }

    /// Returns a generated typed view of the current construct.
    ///
    /// The generated view checks the source snapshot, parse membership,
    /// construct name, and syntax variant before exposing typed fields.
    pub fn typed<T>(&self) -> Result<T, Diagnostic>
    where
        T: TypedConstruct<'source>,
    {
        if self.construct.name != T::CONSTRUCT {
            return Err(Diagnostic::error(
                "LM-TYPED-001",
                format!("expected construct `{}`", T::CONSTRUCT),
                self.source,
                self.construct.span,
            ));
        }

        T::from_validated_construct(self.source, self.construct)
    }

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

    /// Finds the first capture with this name on the current construct or
    /// returns a source-located diagnostic.
    ///
    /// Use [`Self::fields`] when the grammar can capture the same field more
    /// than once.
    pub fn field(&self, name: &str) -> Result<&'source CapturedField, Diagnostic> {
        self.construct
            .fields
            .iter()
            .find(|field| field.name == name)
            .ok_or_else(|| {
                Diagnostic::error(
                    "LM-HANDLER-002",
                    format!(
                        "construct `{}` has no captured field `{name}`",
                        self.construct.name
                    ),
                    self.source,
                    self.construct.span,
                )
            })
    }

    /// Returns every capture with this name from the current construct in
    /// source order.
    ///
    /// This is useful for fields inside repeated grammar patterns. Optional
    /// fields and zero-or-more patterns may have no captures, in which case
    /// the iterator is empty.
    pub fn fields<'iter>(
        &'iter self,
        name: &'iter str,
    ) -> impl Iterator<Item = &'source CapturedField> + 'iter {
        self.construct
            .fields
            .iter()
            .filter(move |field| field.name == name)
    }

    /// Returns the exact source text of the current construct's first capture
    /// with this name.
    ///
    /// Use [`Self::fields`] and [`Self::captured_text`] for repeated captures.
    pub fn field_text(&self, name: &str) -> Result<&'source str, Diagnostic> {
        Ok(self.field_text_capture(name)?.text)
    }

    /// Returns the exact source text and source range of the first capture
    /// with this name.
    ///
    /// Use [`Self::fields`] and [`Self::captured_text`] for repeated captures.
    pub fn field_text_capture(&self, name: &str) -> Result<CapturedText<'source>, Diagnostic> {
        let field = self.field(name)?;

        Ok(CapturedText {
            text: self.captured_text(field)?,
            span: field.span,
        })
    }

    /// Returns the exact source text covered by a captured field.
    pub fn captured_text(&self, field: &CapturedField) -> Result<&'source str, Diagnostic> {
        self.source
            .text()
            .get(field.span.start..field.span.end)
            .ok_or_else(|| {
                Diagnostic::error(
                    "LM-HANDLER-003",
                    format!("captured field `{}` has an invalid source span", field.name),
                    self.source,
                    field.span,
                )
            })
    }

    /// Returns the current construct's identifier capture or a source-located
    /// kind diagnostic.
    pub fn field_identifier(&self, name: &str) -> Result<&'source str, Diagnostic> {
        Ok(self.field_identifier_capture(name)?.text)
    }

    /// Returns an identifier capture together with its source span.
    ///
    /// Use this when language behavior needs both the identifier spelling and
    /// a precise location for name-resolution diagnostics. This does not
    /// resolve the identifier to a declaration.
    pub fn field_identifier_capture(
        &self,
        name: &str,
    ) -> Result<CapturedIdentifier<'source>, Diagnostic> {
        let field = self.field(name)?;

        match &field.value {
            CapturedValue::Identifier(text) => Ok(CapturedIdentifier {
                text,
                span: field.span,
            }),
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
    pub fn dispatch_children(&mut self) -> Result<HandlerControl<Signal>, Diagnostic> {
        self.handlers
            .dispatch_block_children(self.source, self.parsed, self.construct, self.state)
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
