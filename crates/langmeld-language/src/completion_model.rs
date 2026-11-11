use langmeld_core::Span;

/// Semantic category for a completion suggestion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    /// Plain text or an unspecified item.
    Text,
    /// A method.
    Method,
    /// A function.
    Function,
    /// A constructor.
    Constructor,
    /// A field.
    Field,
    /// A variable.
    Variable,
    /// A class.
    Class,
    /// An interface.
    Interface,
    /// A module.
    Module,
    /// A property.
    Property,
    /// A unit.
    Unit,
    /// A value.
    Value,
    /// An enumeration.
    Enum,
    /// A keyword.
    Keyword,
    /// A snippet.
    Snippet,
    /// A color.
    Color,
    /// A file.
    File,
    /// A reference.
    Reference,
    /// A folder.
    Folder,
    /// An enumeration member.
    EnumMember,
    /// A constant.
    Constant,
    /// A structure or record type.
    Struct,
    /// An event.
    Event,
    /// An operator.
    Operator,
    /// A type parameter.
    TypeParameter,
}

impl CompletionKind {
    /// All completion kinds and their canonical `.langmeld` names.
    pub const ALL: [Self; 25] = [
        Self::Text,
        Self::Method,
        Self::Function,
        Self::Constructor,
        Self::Field,
        Self::Variable,
        Self::Class,
        Self::Interface,
        Self::Module,
        Self::Property,
        Self::Unit,
        Self::Value,
        Self::Enum,
        Self::Keyword,
        Self::Snippet,
        Self::Color,
        Self::File,
        Self::Reference,
        Self::Folder,
        Self::EnumMember,
        Self::Constant,
        Self::Struct,
        Self::Event,
        Self::Operator,
        Self::TypeParameter,
    ];

    /// Returns the canonical `.langmeld` spelling for this kind.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Method => "method",
            Self::Function => "function",
            Self::Constructor => "constructor",
            Self::Field => "field",
            Self::Variable => "variable",
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Module => "module",
            Self::Property => "property",
            Self::Unit => "unit",
            Self::Value => "value",
            Self::Enum => "enum",
            Self::Keyword => "keyword",
            Self::Snippet => "snippet",
            Self::Color => "color",
            Self::File => "file",
            Self::Reference => "reference",
            Self::Folder => "folder",
            Self::EnumMember => "enum_member",
            Self::Constant => "constant",
            Self::Struct => "struct",
            Self::Event => "event",
            Self::Operator => "operator",
            Self::TypeParameter => "type_parameter",
        }
    }

    /// Parses a canonical `.langmeld` completion-kind name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == name)
    }
}

/// Parser scope accepted at a source range, used to derive completions.
pub(crate) struct ParseContext {
    pub(crate) span: Span,
    pub(crate) constructs: Vec<String>,
    pub(crate) close: Option<String>,
    pub(crate) values: Vec<String>,
    pub(crate) value_kind: CompletionKind,
    pub(crate) blocked: bool,
}
