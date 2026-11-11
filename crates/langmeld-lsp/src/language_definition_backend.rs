use langmeld_core::SourceFile;
use langmeld_language::{
    CapturedValue, CompletionCandidate, DefinitionFormatOptions, ParseResult, ParsedConstruct,
    complete_at, format_language_definition, language_definition_grammar, parse,
};
use lsp_types::FormattingOptions;

use crate::{
    backend::{BackendCapabilities, BackendDocument, LanguageBackend},
    document_symbol::{DocumentSymbolInfo, DocumentSymbolKind},
};

/// Backend for editing Langmeld's own `.langmeld` language definitions.
#[derive(Clone, Copy, Debug, Default)]
pub struct LanguageDefinitionBackend;

impl LanguageBackend for LanguageDefinitionBackend {
    type Document = ParseResult;

    fn analyze(&self, source: &SourceFile) -> BackendDocument<Self::Document> {
        let parsed = parse(source, language_definition_grammar());
        let diagnostics = parsed.diagnostics().to_vec();

        BackendDocument::new(source, parsed, diagnostics)
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            completion: true,
            formatting: true,
            document_symbols: true,
            ..BackendCapabilities::default()
        }
    }

    fn complete(
        &self,
        source: &SourceFile,
        _document: &BackendDocument<Self::Document>,
        offset: usize,
    ) -> Vec<CompletionCandidate> {
        complete_at(source, language_definition_grammar(), offset)
    }

    fn format_document(
        &self,
        source: &SourceFile,
        _document: &BackendDocument<Self::Document>,
        options: &FormattingOptions,
    ) -> Option<String> {
        format_language_definition(
            source,
            DefinitionFormatOptions {
                tab_size: options.tab_size,
                insert_spaces: options.insert_spaces,
            },
        )
        .ok()
        .filter(|formatted| formatted != source.text())
    }

    fn document_symbols(
        &self,
        _source: &SourceFile,
        document: &BackendDocument<Self::Document>,
    ) -> Vec<DocumentSymbolInfo> {
        document
            .state()
            .constructs()
            .iter()
            .filter_map(definition_document_symbol)
            .collect()
    }
}

fn definition_document_symbol(node: &ParsedConstruct) -> Option<DocumentSymbolInfo> {
    let (field, kind) = match node.name.as_str() {
        "language" => ("name", DocumentSymbolKind::Module),
        "construct" => ("name", DocumentSymbolKind::Struct),
        "import" => ("path", DocumentSymbolKind::File),
        _ => return None,
    };
    let field = node.fields.iter().find(|captured| captured.name == field)?;
    let CapturedValue::String(raw_name) = &field.value else {
        return None;
    };
    let name = decode_definition_string(raw_name)?;
    let children = node
        .children
        .iter()
        .filter_map(definition_document_symbol)
        .collect::<Vec<_>>();
    Some(DocumentSymbolInfo::new(name, kind, node.span, field.span).with_children(children))
}

fn decode_definition_string(value: &str) -> Option<String> {
    let inner = value.strip_prefix('"')?.strip_suffix('"')?;
    let mut decoded = String::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }
        match characters.next()? {
            '\\' => decoded.push('\\'),
            '"' => decoded.push('"'),
            'n' => decoded.push('\n'),
            'r' => decoded.push('\r'),
            't' => decoded.push('\t'),
            _ => return None,
        }
    }
    Some(decoded)
}
