use langmeld_core::{SourceFile, SourceLocation, Span};
use langmeld_language::{LanguageParser, ParsedDocument};
use langmeld_lsp::{
    BackendDocument, CodeActionEdit, CodeActionInfo, DocumentSymbolInfo, DocumentSymbolKind,
    HostLanguageBackend, HoverContent, HoverInfo, LanguageBackend, RenamePreparation,
    SemanticToken, SemanticTokenLegend,
};
use lsp_types::{CodeActionParams, FormattingOptions};

struct Parser;

impl LanguageParser for Parser {
    type Tree = ();

    fn parse(&self, source: &SourceFile) -> ParsedDocument<Self::Tree> {
        ParsedDocument::new(source, (), Vec::new())
    }
}

fn backend_with_providers() -> HostLanguageBackend<'static, Parser> {
    HostLanguageBackend::new(Parser)
        .with_formatter(|_, document, _: &FormattingOptions| {
            document.parsed()?;

            Some("formatted".to_owned())
        })
        .with_hover_provider(|_, document, _| {
            document.parsed()?;

            Some(HoverInfo::plain_text("symbol"))
        })
        .with_semantic_tokens_provider(
            SemanticTokenLegend::new(["variable"], ["declaration"]),
            |source, document| {
                document.parsed().map_or_else(Vec::new, |_| {
                    vec![
                        SemanticToken::new(Span::new(0, source.text().len()), "variable")
                            .with_modifier("declaration"),
                    ]
                })
            },
        )
        .with_document_symbol_provider(|_, document| {
            document.parsed().map_or_else(Vec::new, |_| {
                vec![DocumentSymbolInfo::new(
                    "name",
                    DocumentSymbolKind::Variable,
                    Span::new(0, 4),
                    Span::new(0, 4),
                )]
            })
        })
        .with_code_action_provider(|source, document, _params, range| {
            if document.parsed().is_none() {
                return Vec::new();
            }

            let mut action = CodeActionInfo::new("replace name");
            action.edits.push(CodeActionEdit::new(
                SourceLocation::new(source, range),
                "item",
            ));

            vec![action]
        })
        .with_rename_provider(|source, document, _, _| {
            document.parsed()?;

            Some(vec![SourceLocation::new(source, Span::new(0, 4))])
        })
        .with_prepare_rename_provider(|_, document, _| {
            document.parsed()?;

            Some(RenamePreparation::new(Span::new(0, 4), "name"))
        })
}

fn code_action_params() -> CodeActionParams {
    serde_json::from_value(serde_json::json!({
        "textDocument": { "uri": "file:///main.lang" },
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 4 }
        },
        "context": { "diagnostics": [] }
    }))
    .expect("minimal code action params deserialize")
}

#[test]
fn attached_providers_enable_capabilities_and_receive_validated_documents() {
    let backend = backend_with_providers();
    let source = SourceFile::new("file:///main.lang", "name");
    let document = backend.analyze(&source);
    let capabilities = backend.capabilities();

    assert!(capabilities.formatting);
    assert!(capabilities.hover);
    assert!(capabilities.semantic_tokens);
    assert!(capabilities.document_symbols);
    assert!(capabilities.code_actions);
    assert!(capabilities.rename);
    assert!(capabilities.prepare_rename);
    assert!(!capabilities.definition);

    assert_eq!(
        backend.format_document(&source, &document, &FormattingOptions::default()),
        Some("formatted".to_owned())
    );
    assert_eq!(
        backend
            .hover(&source, &document, 0)
            .map(|hover| hover.content),
        Some(HoverContent::PlainText("symbol".to_owned()))
    );
    assert_eq!(
        backend.semantic_token_legend(),
        SemanticTokenLegend::new(["variable"], ["declaration"])
    );
    assert_eq!(backend.semantic_tokens(&source, &document).len(), 1);
    assert_eq!(backend.document_symbols(&source, &document).len(), 1);
    assert_eq!(
        backend
            .code_actions(&source, &document, &code_action_params(), Span::new(0, 4))
            .len(),
        1
    );
    assert_eq!(
        backend.rename(&source, &document, 0, "item").unwrap().len(),
        1
    );
    assert_eq!(
        backend.prepare_rename(&source, &document, 0),
        Some(RenamePreparation::new(Span::new(0, 4), "name"))
    );
}

#[test]
fn providers_ignore_documents_from_a_different_source_snapshot() {
    let backend = backend_with_providers();
    let source = SourceFile::new("file:///main.lang", "name");
    let other_source = SourceFile::new("file:///other.lang", "name");
    let document = backend.analyze(&source);
    let mismatched_document =
        BackendDocument::new(&other_source, document.into_parts().state, Vec::new());

    assert_eq!(
        backend.format_document(&source, &mismatched_document, &FormattingOptions::default()),
        None
    );
    assert!(backend.hover(&source, &mismatched_document, 0).is_none());
    assert!(
        backend
            .semantic_tokens(&source, &mismatched_document)
            .is_empty()
    );
    assert!(
        backend
            .document_symbols(&source, &mismatched_document)
            .is_empty()
    );
    assert!(
        backend
            .code_actions(
                &source,
                &mismatched_document,
                &code_action_params(),
                Span::new(0, 4)
            )
            .is_empty()
    );
    assert!(
        backend
            .rename(&source, &mismatched_document, 0, "item")
            .is_none()
    );
    assert!(
        backend
            .prepare_rename(&source, &mismatched_document, 0)
            .is_none()
    );
}

#[test]
fn prepare_rename_is_not_advertised_without_rename_support() {
    let backend = HostLanguageBackend::new(Parser).with_prepare_rename_provider(|_, _, _| {
        Some(RenamePreparation::new(Span::new(0, 4), "name"))
    });
    let capabilities = backend.capabilities();

    assert!(!capabilities.rename);
    assert!(!capabilities.prepare_rename);
}
