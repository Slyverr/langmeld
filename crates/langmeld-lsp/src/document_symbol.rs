use langmeld_core::{PositionEncoding, SourceFile, Span};
use lsp_types::{DocumentSymbol, DocumentSymbolResponse, SymbolInformation, Uri};

use crate::conversion::to_lsp_range;

/// Standard LSP category for a document outline symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentSymbolKind {
    /// A file.
    File,
    /// A module.
    Module,
    /// A namespace.
    Namespace,
    /// A package.
    Package,
    /// A class.
    Class,
    /// A method.
    Method,
    /// A property.
    Property,
    /// A field.
    Field,
    /// A constructor.
    Constructor,
    /// An enumeration.
    Enum,
    /// An interface.
    Interface,
    /// A function.
    Function,
    /// A variable or local binding.
    Variable,
    /// A constant.
    Constant,
    /// A string.
    String,
    /// A number.
    Number,
    /// A boolean.
    Boolean,
    /// An array.
    Array,
    /// An object.
    Object,
    /// An object key.
    Key,
    /// A null value.
    Null,
    /// An enumeration member.
    EnumMember,
    /// A struct.
    Struct,
    /// An event.
    Event,
    /// An operator.
    Operator,
    /// A type parameter.
    TypeParameter,
}

/// A host-defined symbol for an editor's document outline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentSymbolInfo {
    /// Display name shown in the outline.
    pub name: String,
    /// Optional signature or type detail.
    pub detail: Option<String>,
    /// LSP symbol category.
    pub kind: DocumentSymbolKind,
    /// UTF-8 byte range enclosing this symbol.
    pub range: Span,
    /// UTF-8 byte range selecting the symbol's name.
    pub selection_range: Span,
    /// Child symbols nested within this symbol.
    pub children: Vec<DocumentSymbolInfo>,
}

impl DocumentSymbolInfo {
    /// Creates a symbol with its full range and name-selection range.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        kind: DocumentSymbolKind,
        range: Span,
        selection_range: Span,
    ) -> Self {
        Self {
            name: name.into(),
            detail: None,
            kind,
            range,
            selection_range,
            children: Vec::new(),
        }
    }

    /// Adds display detail such as a type or function signature.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Adds nested outline entries.
    #[must_use]
    pub fn with_children(mut self, children: impl IntoIterator<Item = Self>) -> Self {
        self.children = children.into_iter().collect();
        self
    }
}

pub(crate) fn convert(
    source: &SourceFile,
    uri: &Uri,
    symbols: Vec<DocumentSymbolInfo>,
    encoding: PositionEncoding,
    hierarchical: bool,
) -> Option<DocumentSymbolResponse> {
    let symbols = symbols
        .into_iter()
        .filter_map(|symbol| convert_symbol(source, symbol, encoding, None))
        .collect::<Vec<_>>();
    if hierarchical {
        Some(DocumentSymbolResponse::Nested(symbols))
    } else {
        let mut flat = Vec::new();
        flatten(&symbols, uri, None, &mut flat);
        Some(DocumentSymbolResponse::Flat(flat))
    }
}

fn convert_symbol(
    source: &SourceFile,
    symbol: DocumentSymbolInfo,
    encoding: PositionEncoding,
    parent: Option<Span>,
) -> Option<DocumentSymbol> {
    if symbol.name.is_empty()
        || !valid_span(source, symbol.range)
        || !valid_span(source, symbol.selection_range)
        || !contains(symbol.range, symbol.selection_range)
        || parent.is_some_and(|parent| !contains(parent, symbol.range))
    {
        return None;
    }
    let range = to_lsp_range(source, symbol.range, encoding)?;
    let selection_range = to_lsp_range(source, symbol.selection_range, encoding)?;
    let children = symbol
        .children
        .into_iter()
        .filter_map(|child| convert_symbol(source, child, encoding, Some(symbol.range)))
        .collect::<Vec<_>>();
    #[expect(
        deprecated,
        reason = "lsp-types 0.97 requires the deprecated field during construction"
    )]
    Some(DocumentSymbol {
        name: symbol.name,
        detail: symbol.detail,
        kind: symbol.kind.into(),
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: (!children.is_empty()).then_some(children),
    })
}

impl From<DocumentSymbolKind> for lsp_types::SymbolKind {
    fn from(kind: DocumentSymbolKind) -> Self {
        match kind {
            DocumentSymbolKind::File => Self::FILE,
            DocumentSymbolKind::Module => Self::MODULE,
            DocumentSymbolKind::Namespace => Self::NAMESPACE,
            DocumentSymbolKind::Package => Self::PACKAGE,
            DocumentSymbolKind::Class => Self::CLASS,
            DocumentSymbolKind::Method => Self::METHOD,
            DocumentSymbolKind::Property => Self::PROPERTY,
            DocumentSymbolKind::Field => Self::FIELD,
            DocumentSymbolKind::Constructor => Self::CONSTRUCTOR,
            DocumentSymbolKind::Enum => Self::ENUM,
            DocumentSymbolKind::Interface => Self::INTERFACE,
            DocumentSymbolKind::Function => Self::FUNCTION,
            DocumentSymbolKind::Variable => Self::VARIABLE,
            DocumentSymbolKind::Constant => Self::CONSTANT,
            DocumentSymbolKind::String => Self::STRING,
            DocumentSymbolKind::Number => Self::NUMBER,
            DocumentSymbolKind::Boolean => Self::BOOLEAN,
            DocumentSymbolKind::Array => Self::ARRAY,
            DocumentSymbolKind::Object => Self::OBJECT,
            DocumentSymbolKind::Key => Self::KEY,
            DocumentSymbolKind::Null => Self::NULL,
            DocumentSymbolKind::EnumMember => Self::ENUM_MEMBER,
            DocumentSymbolKind::Struct => Self::STRUCT,
            DocumentSymbolKind::Event => Self::EVENT,
            DocumentSymbolKind::Operator => Self::OPERATOR,
            DocumentSymbolKind::TypeParameter => Self::TYPE_PARAMETER,
        }
    }
}

fn flatten(
    symbols: &[DocumentSymbol],
    uri: &Uri,
    parent: Option<&str>,
    output: &mut Vec<SymbolInformation>,
) {
    for symbol in symbols {
        #[expect(
            deprecated,
            reason = "lsp-types 0.97 requires the deprecated field during construction"
        )]
        output.push(SymbolInformation {
            name: symbol.name.clone(),
            kind: symbol.kind,
            tags: None,
            deprecated: None,
            location: lsp_types::Location {
                uri: uri.clone(),
                range: symbol.range,
            },
            container_name: parent.map(str::to_owned),
        });
        if let Some(children) = &symbol.children {
            flatten(children, uri, Some(&symbol.name), output);
        }
    }
}

fn valid_span(source: &SourceFile, span: Span) -> bool {
    !span.is_empty()
        && span.end <= source.text.len()
        && source.text.is_char_boundary(span.start)
        && source.text.is_char_boundary(span.end)
}

fn contains(parent: Span, child: Span) -> bool {
    parent.start <= child.start && child.end <= parent.end
}
