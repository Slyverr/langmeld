use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::{ParseResult, ParsedConstruct, ParsedConstructRef};

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

        validate_construct_name::<Self>(source, construct)?;

        Self::from_validated_construct(source, construct)
    }

    /// Creates a typed view from a construct reference yielded by a parse
    /// result, checking its source identity without searching the tree.
    ///
    /// This is useful when walking many constructs: [`Self::from_construct`]
    /// also checks parse-result membership, which requires searching the tree.
    /// A [`ParsedConstructRef`] can only be obtained by iterating a parse
    /// result's constructs or a parent's children.
    ///
    /// # Errors
    ///
    /// Returns a source-located diagnostic when the source snapshot or
    /// construct name does not match this view, or when its captures do not
    /// match the generated view.
    fn from_construct_ref(
        source: &'source SourceFile,
        construct: ParsedConstructRef<'source>,
    ) -> Result<Self, Diagnostic> {
        if construct.source_identity() != source.identity() {
            return Err(Diagnostic::error(
                "LM-TYPED-001",
                "construct reference belongs to a different source snapshot",
                source,
                Span::default(),
            ));
        }

        let construct = construct.construct();
        validate_construct_name::<Self>(source, construct)?;

        Self::from_validated_construct(source, construct)
    }

    /// Creates the view after [`Self::from_construct`] validates its inputs.
    ///
    /// This method is an implementation hook for generated bindings. Callers
    /// should use [`Self::from_construct`], [`Self::from_construct_ref`], or
    /// [`ConstructContext::typed`](crate::ConstructContext::typed).
    #[doc(hidden)]
    fn from_validated_construct(
        source: &'source SourceFile,
        construct: &'source ParsedConstruct,
    ) -> Result<Self, Diagnostic>;
}

fn validate_construct_name<'source, T: TypedConstruct<'source>>(
    source: &'source SourceFile,
    construct: &ParsedConstruct,
) -> Result<(), Diagnostic> {
    if construct.name != T::CONSTRUCT {
        return Err(Diagnostic::error(
            "LM-TYPED-001",
            format!("expected construct `{}`", T::CONSTRUCT),
            source,
            construct.span,
        ));
    }

    Ok(())
}
