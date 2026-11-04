// Generated typed views for a Langmeld language syntax tree.

use std::{collections::BTreeMap, fmt, marker::PhantomData};
use langmeld::core as langmeld_core;
use langmeld::language as langmeld_language;
use langmeld_core::{Diagnostic, Severity, SourceFile, Span};
use langmeld_language::{AstNode, Program as GenericProgram};

pub const LANGUAGE_NAME: &str = "GeneratedConsumer";
pub const LANGUAGE_VERSION: &str = "0.1.0";

/// Reconstructs this module's canonical language definition.
pub fn language_spec() -> langmeld_core::LanguageSpec {
    langmeld_core::LanguageSpec {
        schema_version: 1,
        name: "GeneratedConsumer".to_owned(),
        version: "0.1.0".to_owned(),
        root: "box".to_owned(),
        source_extensions: vec![".lm".to_owned(), ".lmd".to_owned()],
        categories: vec![
            langmeld_core::CategorySpec { id: "statement".to_owned() },
        ],
        rules: vec![
            langmeld_core::SyntaxRuleSpec { id: "item_pattern".to_owned(), parameters: vec![], pattern: "item {name} config {config}".to_owned() },
        ],
        enums: vec![
            langmeld_core::EnumSpec { id: "Mode".to_owned(), values: vec!["fast".to_owned(), "safe".to_owned()] },
        ],
        types: vec![
            langmeld_core::TypeSpec { id: "Coordinates".to_owned(), fields: vec![langmeld_core::FieldSpec { name: "x".to_owned(), kind: langmeld_core::FieldKind::Number, required: true, default: None, constraints: langmeld_core::FieldConstraints { min: None, max: None, min_length: None, max_length: None, min_items: None, max_items: None } }, langmeld_core::FieldSpec { name: "y".to_owned(), kind: langmeld_core::FieldKind::Number, required: true, default: None, constraints: langmeld_core::FieldConstraints { min: None, max: None, min_length: None, max_length: None, min_items: None, max_items: None } }] },
        ],
        behaviors: vec![
            langmeld_core::BehaviorSpec { id: "semantic-check".to_owned(), stage: langmeld_core::BehaviorStage::SemanticAnalysis },
            langmeld_core::BehaviorSpec { id: "symbol-check".to_owned(), stage: langmeld_core::BehaviorStage::SymbolResolution },
            langmeld_core::BehaviorSpec { id: "type-check".to_owned(), stage: langmeld_core::BehaviorStage::WorkspaceTypeAnalysis },
        ],
        constructs: vec![
            langmeld_core::ConstructSpec {
                id: "box".to_owned(),
                implements: vec![],
                custom_parser: None,
                syntax: vec!["box {namespace} {".to_owned()],
                syntax_ids: vec!["root".to_owned()],
                fields: vec![langmeld_core::FieldSpec { name: "namespace".to_owned(), kind: langmeld_core::FieldKind::Identifier, required: true, default: None, constraints: langmeld_core::FieldConstraints { min: None, max: None, min_length: None, max_length: None, min_items: None, max_items: None } }],
                block: Some(langmeld_core::BlockSpec { allowed: vec!["statement".to_owned()], open: "{".to_owned(), close: "}".to_owned(), separators: vec![langmeld_core::BlockSeparator::Newline, langmeld_core::BlockSeparator::Literal(";".to_owned())], separator_policy: langmeld_core::BlockSeparatorPolicy::Optional }),
                symbol: Some(langmeld_core::SymbolSpec { kind: "namespace".to_owned(), name_field: "namespace".to_owned(), scope: None }),
            },
            langmeld_core::ConstructSpec {
                id: "item".to_owned(),
                implements: vec!["statement".to_owned()],
                custom_parser: None,
                syntax: vec!["item {name} config {config}".to_owned()],
                syntax_ids: vec!["item".to_owned()],
                fields: vec![langmeld_core::FieldSpec { name: "name".to_owned(), kind: langmeld_core::FieldKind::Identifier, required: true, default: None, constraints: langmeld_core::FieldConstraints { min: None, max: None, min_length: None, max_length: None, min_items: None, max_items: None } }, langmeld_core::FieldSpec { name: "config".to_owned(), kind: langmeld_core::FieldKind::Properties { name: None, fields: vec![langmeld_core::FieldSpec { name: "title".to_owned(), kind: langmeld_core::FieldKind::Text, required: true, default: None, constraints: langmeld_core::FieldConstraints { min: None, max: None, min_length: None, max_length: None, min_items: None, max_items: None } }, langmeld_core::FieldSpec { name: "mode".to_owned(), kind: langmeld_core::FieldKind::NamedEnum { name: "Mode".to_owned(), values: vec!["fast".to_owned(), "safe".to_owned()] }, required: true, default: None, constraints: langmeld_core::FieldConstraints { min: None, max: None, min_length: None, max_length: None, min_items: None, max_items: None } }, langmeld_core::FieldSpec { name: "tags".to_owned(), kind: langmeld_core::FieldKind::List { item: std::boxed::Box::new(langmeld_core::FieldKind::Text) }, required: true, default: None, constraints: langmeld_core::FieldConstraints { min: None, max: None, min_length: None, max_length: None, min_items: None, max_items: None } }, langmeld_core::FieldSpec { name: "attrs".to_owned(), kind: langmeld_core::FieldKind::Map { key: langmeld_core::MapKeyKind::Identifier, value: std::boxed::Box::new(langmeld_core::FieldKind::Text) }, required: true, default: None, constraints: langmeld_core::FieldConstraints { min: None, max: None, min_length: None, max_length: None, min_items: None, max_items: None } }] }, required: true, default: None, constraints: langmeld_core::FieldConstraints { min: None, max: None, min_length: None, max_length: None, min_items: None, max_items: None } }],
                block: None,
                symbol: Some(langmeld_core::SymbolSpec { kind: "item".to_owned(), name_field: "name".to_owned(), scope: Some(langmeld_core::ScopeSpec { construct: "box".to_owned(), name_field: "namespace".to_owned(), lookup: langmeld_core::ScopeLookup::LexicalParents }) }),
            },
        ],
    }
}

/// Validates the embedded language definition.
pub fn validated_language() -> Result<langmeld_core::ValidatedLanguageSpec, Vec<langmeld_core::SpecError>> {
    language_spec().validate()
}

/// Captured values accepted by generated property records.
pub type PropertyMap = BTreeMap<String, langmeld_language::CapturedValue>;

#[derive(Clone, Debug, PartialEq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

/// A reference spelling parameterized by the generated declaration type.
#[derive(Clone, Debug, PartialEq)]
pub struct Reference<T> {
    pub name: String,
    _target: PhantomData<fn() -> T>,
}
impl<T> Reference<T> {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), _target: PhantomData }
    }
    #[must_use]
    pub fn as_str(&self) -> &str { &self.name }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConversionError {
    WrongConstruct { expected: &'static str, actual: String },
    MissingField(&'static str),
    WrongFieldKind(&'static str),
    UnknownEnumValue { field: &'static str, value: String },
    UnknownChild { parent: &'static str, child: String },
    UnexpectedChildren { parent: &'static str },
    InvalidSyntaxVariant { construct: &'static str, variant: String },
}
impl fmt::Display for ConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongConstruct { expected, actual } => write!(formatter, "expected construct {expected}, got {actual}"),
            Self::MissingField(field) => write!(formatter, "required field {field} is absent"),
            Self::WrongFieldKind(field) => write!(formatter, "field {field} has an unexpected value kind"),
            Self::UnknownEnumValue { field, value } => write!(formatter, "field {field} has unknown enum value {value}"),
            Self::UnknownChild { parent, child } => write!(formatter, "construct {child} is not an allowed child of {parent}"),
            Self::UnexpectedChildren { parent } => write!(formatter, "construct {parent} cannot have children"),
            Self::InvalidSyntaxVariant { construct, variant } => write!(formatter, "construct {construct} has unknown syntax variant {variant}"),
        }
    }
}
impl std::error::Error for ConversionError {}

/// Values accepted by this generated field type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Fast,
    Safe,
}
impl Mode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Safe => "safe",
        }
    }
}

/// Typed collection captured by `item.config.tags`.
pub type ItemConfigTagsCollection = Vec<Spanned<String>>;

/// Typed collection captured by `item.config.attrs`.
pub type ItemConfigAttrsCollection = BTreeMap<String, Spanned<String>>;

/// Typed property record captured by `Coordinates`.
#[derive(Clone, Debug, PartialEq)]
pub struct Coordinates {
    pub x: Spanned<f64>,
    pub y: Spanned<f64>,
}

impl TryFrom<&PropertyMap> for Coordinates {
    type Error = ConversionError;
    fn try_from(values: &PropertyMap) -> Result<Self, Self::Error> {
        let x = read_value_3(values.get("x").ok_or(ConversionError::MissingField("x"))?)?;
        let y = read_value_4(values.get("y").ok_or(ConversionError::MissingField("y"))?)?;
        Ok(Self {
            x,
            y,
        })
    }
}

/// Typed property record captured by `item.config`.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemConfig {
    pub title: Spanned<String>,
    pub mode: Spanned<Mode>,
    pub tags: Spanned<ItemConfigTagsCollection>,
    pub attrs: Spanned<ItemConfigAttrsCollection>,
}

impl TryFrom<&PropertyMap> for ItemConfig {
    type Error = ConversionError;
    fn try_from(values: &PropertyMap) -> Result<Self, Self::Error> {
        let title = read_value_5(values.get("title").ok_or(ConversionError::MissingField("title"))?)?;
        let mode = read_value_6(values.get("mode").ok_or(ConversionError::MissingField("mode"))?)?;
        let tags = read_value_7(values.get("tags").ok_or(ConversionError::MissingField("tags"))?)?;
        let attrs = read_value_9(values.get("attrs").ok_or(ConversionError::MissingField("attrs"))?)?;
        Ok(Self {
            title,
            mode,
            tags,
            attrs,
        })
    }
}

/// Child nodes accepted by `box`.
#[derive(Clone, Debug, PartialEq)]
pub enum BoxChild {
    Item(Item),
}

fn read_value_0(captured: &langmeld_language::CapturedValue) -> Result<Spanned<String>, ConversionError> {
    let value = match &captured.value { langmeld_language::Value::Identifier(value) => value.clone(), _ => return Err(ConversionError::WrongFieldKind("namespace")) };
    Ok(Spanned { value, span: captured.span })
}

fn read_value_1(captured: &langmeld_language::CapturedValue) -> Result<Spanned<String>, ConversionError> {
    let value = match &captured.value { langmeld_language::Value::Identifier(value) => value.clone(), _ => return Err(ConversionError::WrongFieldKind("name")) };
    Ok(Spanned { value, span: captured.span })
}

fn read_value_2(captured: &langmeld_language::CapturedValue) -> Result<Spanned<ItemConfig>, ConversionError> {
    let values = match &captured.value { langmeld_language::Value::Properties(values) => values, _ => return Err(ConversionError::WrongFieldKind("config")) };
    let value = ItemConfig::try_from(values)?;
    Ok(Spanned { value, span: captured.span })
}

fn read_value_3(captured: &langmeld_language::CapturedValue) -> Result<Spanned<f64>, ConversionError> {
    let value = match &captured.value { langmeld_language::Value::Number(value) => *value, _ => return Err(ConversionError::WrongFieldKind("x")) };
    Ok(Spanned { value, span: captured.span })
}

fn read_value_4(captured: &langmeld_language::CapturedValue) -> Result<Spanned<f64>, ConversionError> {
    let value = match &captured.value { langmeld_language::Value::Number(value) => *value, _ => return Err(ConversionError::WrongFieldKind("y")) };
    Ok(Spanned { value, span: captured.span })
}

fn read_value_5(captured: &langmeld_language::CapturedValue) -> Result<Spanned<String>, ConversionError> {
    let value = match &captured.value { langmeld_language::Value::Text(value) => value.clone(), _ => return Err(ConversionError::WrongFieldKind("title")) };
    Ok(Spanned { value, span: captured.span })
}

fn read_value_6(captured: &langmeld_language::CapturedValue) -> Result<Spanned<Mode>, ConversionError> {
    let value = match &captured.value {
        langmeld_language::Value::Enum(value) => match value.as_str() {
            "fast" => Mode::Fast,
            "safe" => Mode::Safe,
            other => return Err(ConversionError::UnknownEnumValue { field: "mode", value: other.to_owned() }),
        },
        _ => return Err(ConversionError::WrongFieldKind("mode")),
    };
    Ok(Spanned { value, span: captured.span })
}

fn read_value_7(captured: &langmeld_language::CapturedValue) -> Result<Spanned<ItemConfigTagsCollection>, ConversionError> {
    let values = match &captured.value { langmeld_language::Value::List(values) => values, _ => return Err(ConversionError::WrongFieldKind("tags")) };
    let value = values.iter().map(read_value_8).collect::<Result<Vec<_>, ConversionError>>()?;
    Ok(Spanned { value, span: captured.span })
}

fn read_value_8(captured: &langmeld_language::CapturedValue) -> Result<Spanned<String>, ConversionError> {
    let value = match &captured.value { langmeld_language::Value::Text(value) => value.clone(), _ => return Err(ConversionError::WrongFieldKind("tags")) };
    Ok(Spanned { value, span: captured.span })
}

fn read_value_9(captured: &langmeld_language::CapturedValue) -> Result<Spanned<ItemConfigAttrsCollection>, ConversionError> {
    let entries = match &captured.value { langmeld_language::Value::Map(entries) => entries, _ => return Err(ConversionError::WrongFieldKind("attrs")) };
    let value = entries.iter().map(|entry| Ok((entry.key.clone(), read_value_10(&entry.value)?))).collect::<Result<BTreeMap<_, _>, ConversionError>>()?;
    Ok(Spanned { value, span: captured.span })
}

fn read_value_10(captured: &langmeld_language::CapturedValue) -> Result<Spanned<String>, ConversionError> {
    let value = match &captured.value { langmeld_language::Value::Text(value) => value.clone(), _ => return Err(ConversionError::WrongFieldKind("attrs")) };
    Ok(Spanned { value, span: captured.span })
}

/// Typed form of the `box` construct.
#[derive(Clone, Debug, PartialEq)]
pub struct Box {
    pub namespace: Spanned<String>,
    pub children: Vec<BoxChild>,
    pub syntax_variant: String,
    pub span: Span,
}

impl TryFrom<&AstNode> for Box {
    type Error = ConversionError;
    fn try_from(node: &AstNode) -> Result<Self, Self::Error> {
        if node.construct != "box" { return Err(ConversionError::WrongConstruct { expected: "box", actual: node.construct.clone() }); }
        if !matches!(node.syntax_variant.as_str(), "root") { return Err(ConversionError::InvalidSyntaxVariant { construct: "box", variant: node.syntax_variant.clone() }); }
        let namespace = read_value_0(node.fields.get("namespace").ok_or(ConversionError::MissingField("namespace"))?)?;
        let children = node.children.iter().map(|child| match child.construct.as_str() {
            "item" => Ok(BoxChild::Item(Item::try_from(child)?)),
            child => Err(ConversionError::UnknownChild { parent: "box", child: child.to_owned() }),
        }).collect::<Result<Vec<_>, ConversionError>>()?;
        Ok(Self {
            namespace,
            children,
            syntax_variant: node.syntax_variant.clone(),
            span: node.span,
        })
    }
}

/// Typed form of the `item` construct.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub name: Spanned<String>,
    pub config: Spanned<ItemConfig>,
    pub syntax_variant: String,
    pub span: Span,
}

impl TryFrom<&AstNode> for Item {
    type Error = ConversionError;
    fn try_from(node: &AstNode) -> Result<Self, Self::Error> {
        if node.construct != "item" { return Err(ConversionError::WrongConstruct { expected: "item", actual: node.construct.clone() }); }
        if !matches!(node.syntax_variant.as_str(), "item") { return Err(ConversionError::InvalidSyntaxVariant { construct: "item", variant: node.syntax_variant.clone() }); }
        let name = read_value_1(node.fields.get("name").ok_or(ConversionError::MissingField("name"))?)?;
        let config = read_value_2(node.fields.get("config").ok_or(ConversionError::MissingField("config"))?)?;
        if !node.children.is_empty() { return Err(ConversionError::UnexpectedChildren { parent: "item" }); }
        Ok(Self {
            name,
            config,
            syntax_variant: node.syntax_variant.clone(),
            span: node.span,
        })
    }
}

/// Converts generic root nodes into generated typed syntax values.
pub fn from_program(program: &GenericProgram) -> Result<Vec<Box>, ConversionError> {
    program.nodes.iter().map(Box::try_from).collect()
}

/// Parses and analyzes source while preserving non-error diagnostics.
pub fn parse_source(source: &SourceFile) -> Result<(Vec<Box>, Vec<Diagnostic>), Vec<Diagnostic>> {
    parse_source_with_custom_parsers(source, &[])
}

/// Parses source with the callbacks required by this language definition.
pub fn parse_source_with_custom_parsers(
    source: &SourceFile,
    custom_parsers: &[langmeld_language::CustomConstructParser],
) -> Result<(Vec<Box>, Vec<Diagnostic>), Vec<Diagnostic>> {
    let language = validated_language().map_err(|errors| {
        errors.into_iter().map(|error| Diagnostic::spec_error(error.code, error.message)).collect::<Vec<_>>()
    })?;
    let prepared = langmeld_language::PreparedLanguage::new(language.clone())
        .map_err(|error| vec![Diagnostic::spec_error("GEN0002", error.to_string())])?;
    let parser = prepared.with_custom_parsers(custom_parsers.iter().cloned())
        .map_err(|error| vec![Diagnostic::spec_error("GEN0003", error.to_string())])?;
    let parsed = parser.parse(source);
    let mut diagnostics = parsed.diagnostics;
    diagnostics.extend(langmeld_language::analyze(&language, &parsed.program, source));
    if diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error) {
        return Err(diagnostics);
    }
    let roots = from_program(&parsed.program)
        .map_err(|error| vec![Diagnostic::spec_error("GEN0001", error.to_string())])?;
    Ok((roots, diagnostics))
}

