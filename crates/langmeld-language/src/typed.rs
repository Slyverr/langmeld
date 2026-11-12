use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::{ParseResult, ParsedConstruct};

/// A generated typed view over one parsed construct.
///
/// Implementations are normally emitted by `langmeld-build`. The view borrows
/// the parse result and source snapshot; it does not copy or own parsed data.
pub trait TypedConstruct<'source>: Sized {
    /// Construct name declared by the language definition.
    const CONSTRUCT: &'static str;

    /// Creates a typed view for a construct from the parse result that owns it.
    ///
    /// # Errors
    ///
    /// Returns a source-located diagnostic when the source snapshot, parse
    /// result, or construct does not match this view.
    fn from_construct(
        source: &'source SourceFile,
        parsed: &'source ParseResult,
        construct: &'source ParsedConstruct,
    ) -> Result<Self, Diagnostic> {
        if parsed.source_identity() != source.identity() {
            return Err(Diagnostic::error(
                "LM-TYPED-001",
                "parse result belongs to a different source snapshot",
                source,
                Span::default(),
            ));
        }
        if !parsed.contains_construct(construct) {
            return Err(Diagnostic::error(
                "LM-TYPED-001",
                "construct does not belong to the supplied parse result",
                source,
                Span::default(),
            ));
        }
        if construct.name != Self::CONSTRUCT {
            return Err(Diagnostic::error(
                "LM-TYPED-001",
                format!("expected construct `{}`", Self::CONSTRUCT),
                source,
                construct.span,
            ));
        }

        Self::from_validated_construct(source, construct)
    }

    /// Creates the view after [`Self::from_construct`] validates its inputs.
    ///
    /// This method is an implementation hook for generated bindings. Callers
    /// should use [`Self::from_construct`] or [`ConstructContext::typed`](crate::ConstructContext::typed).
    #[doc(hidden)]
    fn from_validated_construct(
        source: &'source SourceFile,
        construct: &'source ParsedConstruct,
    ) -> Result<Self, Diagnostic>;
}
