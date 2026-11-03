use crate::analysis::ValueType;
use langmeld_core::{Diagnostic, SourceFile, Span};
use std::{any::Any, collections::BTreeMap, fmt, sync::Arc};

/// A parsed language program. Nodes preserve source ranges but not comments or whitespace.
#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    /// Top-level syntax nodes.
    pub nodes: Vec<AstNode>,
}

/// Best-effort parse output, including a lossless source tree and token stream.
#[derive(Clone, Debug, PartialEq)]
pub struct ParseResult {
    /// Generic syntax nodes recovered from the source.
    pub program: Program,
    /// Parent-aware tree containing constructs, field values, and token leaves.
    /// Its root span covers the complete original source snapshot.
    pub syntax_tree: crate::SyntaxNode,
    /// Lossless source segments. Non-empty spans partition the original source;
    /// zero-width entries represent synthetic recovery tokens.
    pub syntax_tokens: Vec<crate::SyntaxToken>,
    /// Lexical and syntactic diagnostics, including recoverable errors.
    pub diagnostics: Vec<Diagnostic>,
    /// Source ranges recognized by host-provided construct parsers.
    pub custom_parser_spans: Vec<Span>,
    /// Parser-derived completion expectations for incomplete declarative syntax.
    pub completion_frontiers: Vec<CompletionFrontier>,
    /// Active child-construct contexts inferred while parsing declarative blocks.
    pub completion_blocks: Vec<CompletionBlockContext>,
    /// Whether a source, token, AST, nesting, or matcher-work limit stopped parsing.
    pub resource_limited: bool,
}

/// Expected captures at the furthest valid prefix of one incomplete construct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionFrontier {
    /// Source range of the token or zero-width cursor position at the frontier.
    pub span: Span,
    /// Whether equally advanced grammar branches disagree about expected fields.
    pub ambiguous: bool,
    /// Schema fields that can continue this prefix.
    pub captures: Vec<ExpectedCapture>,
    /// Bounded, unambiguous continuation from this point in a simple pattern.
    pub continuation: Option<Vec<CompletionPart>>,
}

/// One literal or field placeholder in a parser-derived completion continuation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompletionPart {
    /// Literal source text required by the compiled pattern.
    Literal(String),
    /// A schema field to insert as an editable snippet placeholder.
    Capture(String),
}

/// One declared field expected at an incomplete grammar frontier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedCapture {
    /// Construct whose syntax pattern expects the field.
    pub construct: String,
    /// Syntax variant that reached this expectation.
    pub syntax_variant: String,
    /// Field or property name expected by the pattern.
    pub field: String,
    /// Declared type of the expected capture.
    pub kind: langmeld_core::FieldKind,
}

/// Allowed child constructs at a cursor position inside a parsed block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionBlockContext {
    /// Parent construct that declares this block.
    pub parent: String,
    /// Construct IDs or categories accepted by the block.
    pub allowed: Vec<String>,
    /// Source range from the previous child through the current child prefix.
    pub span: Span,
}

/// A generic syntax node produced from one declared construct.
#[derive(Clone, Debug, PartialEq)]
pub struct AstNode {
    /// Construct identifier from the language definition.
    pub construct: String,
    /// Stable syntax-variant identifier from the language definition.
    pub syntax_variant: String,
    /// Captured fields indexed by their declared names.
    pub fields: BTreeMap<String, CapturedValue>,
    /// Nested syntax nodes.
    pub children: Vec<AstNode>,
    /// Source extent of this node including its closing brace if it has a body.
    pub span: Span,
    /// Optional typed host syntax or semantic data attached by a custom parser.
    pub host_data: Option<HostData>,
}

impl AstNode {
    /// Creates an empty host-built syntax node.
    #[must_use]
    pub fn new(
        construct: impl Into<String>,
        syntax_variant: impl Into<String>,
        span: Span,
    ) -> Self {
        Self {
            construct: construct.into(),
            syntax_variant: syntax_variant.into(),
            fields: BTreeMap::new(),
            children: Vec::new(),
            span,
            host_data: None,
        }
    }

    /// Adds or replaces a captured field.
    #[must_use]
    pub fn with_field(mut self, name: impl Into<String>, value: CapturedValue) -> Self {
        self.fields.insert(name.into(), value);
        self
    }

    /// Sets the node's nested host-built syntax nodes.
    #[must_use]
    pub fn with_children(mut self, children: impl IntoIterator<Item = AstNode>) -> Self {
        self.children = children.into_iter().collect();
        self
    }

    /// Attaches an immutable host-owned value to this node.
    #[must_use]
    pub fn with_host_data<T: Any + Send + Sync + 'static>(mut self, data: T) -> Self {
        self.host_data = Some(HostData::new(data));
        self
    }

    /// Returns the host-owned value when it has the requested concrete type.
    #[must_use]
    pub fn host_data<T: Any + Send + Sync + 'static>(&self) -> Option<&T> {
        self.host_data.as_ref()?.downcast_ref()
    }
}

/// An immutable, type-erased value owned by a language host.
///
/// Langmeld clones this shared handle with syntax nodes and does not inspect the
/// value. A host can use it to retain native AST nodes, type information, or other
/// parser data while Langmeld operates on the node's generic projection. Equality
/// compares shared handle identity, since the wrapped type may not implement `Eq`.
#[derive(Clone)]
pub struct HostData {
    value: Arc<dyn Any + Send + Sync>,
    type_name: &'static str,
}

impl HostData {
    /// Wraps a host-owned value for attachment to a parsed node.
    #[must_use]
    pub fn new<T: Any + Send + Sync + 'static>(value: T) -> Self {
        Self {
            value: Arc::new(value),
            type_name: std::any::type_name::<T>(),
        }
    }

    /// Borrows the wrapped value when it has the requested concrete type.
    #[must_use]
    pub fn downcast_ref<T: Any + Send + Sync + 'static>(&self) -> Option<&T> {
        self.value.downcast_ref()
    }

    /// Returns the Rust type name recorded when this value was wrapped.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        self.type_name
    }
}

impl fmt::Debug for HostData {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HostData")
            .field("type_name", &self.type_name)
            .finish_non_exhaustive()
    }
}

impl PartialEq for HostData {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.value, &other.value)
    }
}

impl Eq for HostData {}

/// A captured field value and its source range.
#[derive(Clone, Debug, PartialEq)]
pub struct CapturedValue {
    /// Parsed value category and payload.
    pub value: Value,
    /// Token range that supplied the value.
    pub span: Span,
}

impl CapturedValue {
    /// Associates a parsed value with the source range that produced it.
    #[must_use]
    pub const fn new(value: Value, span: Span) -> Self {
        Self { value, span }
    }
}

/// One map key/value pair with source locations for both sides.
#[derive(Clone, Debug, PartialEq)]
pub struct MapEntry {
    /// Normalized key contents.
    pub key: String,
    /// Lexical category of the key token.
    pub key_type: ValueType,
    /// Source range of the key token.
    pub key_span: Span,
    /// Parsed value and its source range.
    pub value: CapturedValue,
}

/// One construct recognized by a host-provided parser callback.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomConstructMatch {
    /// Complete construct header range, including an opening brace for blocks.
    pub span: Span,
    /// Values captured by the custom parser using the definition's field names.
    pub fields: BTreeMap<String, CapturedValue>,
    /// Child syntax nodes supplied by the host parser for layout-sensitive or
    /// otherwise custom grammar. `None` leaves block parsing to the generic parser.
    pub children: Option<Vec<AstNode>>,
    /// Stable syntax variant identifier used by tooling and generated ASTs.
    pub syntax_variant: String,
    /// Recoverable parser diagnostics to retain alongside the returned syntax tree.
    /// Every span must be a valid UTF-8 byte range in the source file.
    pub diagnostics: Vec<Diagnostic>,
    /// Optional native parser result retained on the resulting [`AstNode`].
    pub host_data: Option<HostData>,
}

impl CustomConstructMatch {
    /// Creates a host parser result with no captured fields or custom children.
    #[must_use]
    pub fn new(span: Span, syntax_variant: impl Into<String>) -> Self {
        Self {
            span,
            fields: BTreeMap::new(),
            children: None,
            syntax_variant: syntax_variant.into(),
            diagnostics: Vec::new(),
            host_data: None,
        }
    }

    /// Adds or replaces a captured field.
    #[must_use]
    pub fn with_field(mut self, name: impl Into<String>, value: CapturedValue) -> Self {
        self.fields.insert(name.into(), value);
        self
    }

    /// Supplies the complete child list, including an explicitly empty list.
    #[must_use]
    pub fn with_children(mut self, children: impl IntoIterator<Item = AstNode>) -> Self {
        self.children = Some(children.into_iter().collect());
        self
    }

    /// Adds a recoverable diagnostic to retain alongside this parsed match.
    #[must_use]
    pub fn with_diagnostic(mut self, diagnostic: Diagnostic) -> Self {
        self.diagnostics.push(diagnostic);
        self
    }

    /// Attaches a native host AST node or another immutable parser result.
    #[must_use]
    pub fn with_host_data<T: Any + Send + Sync + 'static>(mut self, data: T) -> Self {
        self.host_data = Some(HostData::new(data));
        self
    }
}

/// Host callback used for a construct marked with `custom_parser` in its definition.
/// Return `Ok(None)` only when the input is not this construct; return a diagnostic
/// when the callback recognizes the construct but cannot produce a usable match.
/// A usable partial match can carry recoverable diagnostics beside its tree.
pub type CustomConstructParserFn =
    dyn Fn(&SourceFile, usize) -> Result<Option<CustomConstructMatch>, Diagnostic> + Send + Sync;

/// Registers a host parser callback under the name used in a language definition.
#[derive(Clone)]
pub struct CustomConstructParser {
    /// Registration name referenced by `ConstructSpec::custom_parser`.
    pub id: String,
    /// Parses a construct at the supplied UTF-8 byte offset.
    pub parse: Arc<CustomConstructParserFn>,
}

impl CustomConstructParser {
    /// Creates a parser registration backed by a function or configured closure.
    ///
    /// Configured closures let an embedder bind project-specific state such as
    /// compiler options, feature flags, or an external parser instance.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        parse: impl Fn(&SourceFile, usize) -> Result<Option<CustomConstructMatch>, Diagnostic>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            parse: Arc::new(parse),
        }
    }
}

impl fmt::Debug for CustomConstructParser {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CustomConstructParser")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// Built-in values available to typed accessors and generic analysis.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Identifier name.
    Identifier(String),
    /// Finite number.
    Number(f64),
    /// Quoted string contents.
    Text(String),
    /// A member of a declared enum field.
    Enum(String),
    /// Reference spelling before name resolution.
    Reference(String),
    /// Typed named values parsed from a delimited property block.
    Properties(BTreeMap<String, CapturedValue>),
    /// Typed ordered values parsed from a bracket-delimited list.
    List(Vec<CapturedValue>),
    /// Typed key/value pairs parsed from a brace-delimited map.
    Map(Vec<MapEntry>),
}
