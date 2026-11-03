//! Canonical, serializable language-definition data types.

use serde::{Deserialize, Serialize};

/// The canonical, serializable description of one language.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageSpec {
    /// Version of the bootstrap JSON schema.
    pub schema_version: u32,
    /// Human-readable language name.
    pub name: String,
    /// Language definition version.
    pub version: String,
    /// Construct that must appear at the top level of a program.
    pub root: String,
    /// Source filename suffixes accepted by the language, including the leading dot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_extensions: Vec<String>,
    /// Reusable kinds that constructs may implement and blocks may accept.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<CategorySpec>,
    /// Named syntax fragments expanded into construct patterns during validation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<SyntaxRuleSpec>,
    /// Reusable enumerations that fields can reference by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enums: Vec<EnumSpec>,
    /// Named record schemas that definitions can reuse as field types.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<TypeSpec>,
    /// Host behavior stages required by this language.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub behaviors: Vec<BehaviorSpec>,
    /// Constructs available in this language.
    pub constructs: Vec<ConstructSpec>,
}

/// A reusable record schema for values captured from source syntax.
///
/// This describes the shape of parsed data; it does not define a language's
/// runtime types, expression typing, or execution semantics.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TypeSpec {
    /// Stable type identifier.
    pub id: String,
    /// Fields captured from a delimited property record.
    pub fields: Vec<FieldSpec>,
}

/// A reusable finite set of identifier values.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnumSpec {
    /// Stable enumeration identifier.
    pub id: String,
    /// Allowed values in their declared order.
    pub values: Vec<String>,
}

/// A reusable syntax fragment referenced as `@id` from a construct pattern.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SyntaxRuleSpec {
    /// Stable identifier used in syntax patterns.
    pub id: String,
    /// Placeholder names supplied by each use of this rule.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<String>,
    /// Pattern fragment; formal captures map to field names at each use.
    pub pattern: String,
}

/// A reusable category that defines an extension point for block contents.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CategorySpec {
    /// Stable category identifier.
    pub id: String,
}

/// A host-provided behavior required by a language definition.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorSpec {
    /// Stable callback identity registered by the host.
    pub id: String,
    /// Pipeline stage at which the callback runs.
    pub stage: BehaviorStage,
}

/// Supported host behavior stages.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorStage {
    /// Per-file host validation after parsing and generic name analysis.
    SemanticAnalysis,
    /// Workspace-wide host name resolution replacing generic symbol matching.
    SymbolResolution,
    /// Workspace-wide checks over resolved declarations, references, and type facts.
    WorkspaceTypeAnalysis,
}

/// A syntax construct with patterns, fields, optional children, and symbol rules.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructSpec {
    /// Stable identifier used in syntax trees and other declarations.
    pub id: String,
    /// Categories implemented by this construct.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implements: Vec<String>,
    /// Optional host parser registration used before the declarative patterns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_parser: Option<String>,
    /// Whitespace-insensitive token templates, such as `node {name} at ({x}, {y})`.
    pub syntax: Vec<String>,
    /// Optional stable names for syntax patterns, in the same order as `syntax`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub syntax_ids: Vec<String>,
    /// Values captured by this construct's syntax patterns.
    #[serde(default)]
    pub fields: Vec<FieldSpec>,
    /// Child constructs accepted inside a brace-delimited body.
    #[serde(default)]
    pub block: Option<BlockSpec>,
    /// Optional symbol declaration rule.
    #[serde(default)]
    pub symbol: Option<SymbolSpec>,
}

/// A field captured from a construct pattern.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct FieldSpec {
    /// Field name referenced by `{name}` in a syntax pattern.
    pub name: String,
    /// Expected token category for the captured value.
    #[serde(flatten)]
    pub kind: FieldKind,
    /// Whether the syntax tree always contains this field after defaults are applied.
    #[serde(default = "default_true")]
    pub required: bool,
    /// Typed scalar value, encoded as text, used when syntax omits this field.
    /// References cannot have defaults; validation interprets this by `kind`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Optional scalar or collection constraints.
    #[serde(default, skip_serializing_if = "FieldConstraints::is_empty")]
    pub constraints: FieldConstraints,
}

/// Constraints enforced after a value is parsed.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FieldConstraints {
    /// Inclusive integer lower bound for number fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    /// Inclusive integer upper bound for number fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    /// Minimum Unicode scalar count for text fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_length: Option<usize>,
    /// Maximum Unicode scalar count for text fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
    /// Minimum number of entries for list, map, or property-record fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_items: Option<usize>,
    /// Maximum number of entries for list, map, or property-record fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<usize>,
}

impl FieldConstraints {
    /// Returns whether all constraints are unset.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.min.is_none()
            && self.max.is_none()
            && self.min_length.is_none()
            && self.max_length.is_none()
            && self.min_items.is_none()
            && self.max_items.is_none()
    }
}

const fn default_true() -> bool {
    true
}

/// Built-in field types supported by the shared declarative parser.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FieldKind {
    /// An identifier token.
    Identifier,
    /// A finite decimal number token.
    Number,
    /// A quoted string token.
    Text,
    /// An identifier constrained to a declared set of values.
    Enum {
        /// Allowed values.
        values: Vec<String>,
    },
    /// An identifier constrained by a declared reusable enumeration.
    NamedEnum {
        /// Stable identifier of the enumeration declaration.
        #[serde(rename = "enum_name")]
        name: String,
        /// Allowed values copied from the declaration for local validation.
        values: Vec<String>,
    },
    /// An identifier that must resolve to a declaration of this symbol kind.
    Reference {
        /// Symbol kind to resolve against, such as `node`.
        target: String,
    },
    /// A delimited record of named fields, usually shared through an `.lm` properties bundle.
    Properties {
        /// Optional declared reusable record type name; inline property blocks omit it.
        #[serde(default, rename = "record", skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Fields parsed between the property block's braces.
        fields: Vec<FieldSpec>,
    },
    /// A bracket-delimited sequence of values sharing one field type.
    List {
        /// Type of every element in the list.
        item: Box<FieldKind>,
    },
    /// A delimited set of unique keys and homogeneous scalar values.
    Map {
        /// Lexical key type accepted in source.
        key: MapKeyKind,
        /// Type of every value in the map.
        value: Box<FieldKind>,
    },
}

/// Key spelling supported by a declarative map field.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MapKeyKind {
    /// Unquoted identifier keys, such as `owner`.
    Identifier,
    /// Quoted string keys, such as `"build target"`.
    Text,
}

/// Child construct kinds permitted in a construct body.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BlockSpec {
    /// IDs of constructs accepted in this body.
    pub allowed: Vec<String>,
}

/// Symbol declaration metadata for generic duplicate and reference checks.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolSpec {
    /// Name used by reference fields to identify this symbol class.
    pub kind: String,
    /// Identifier field that provides the declared name.
    pub name_field: String,
    /// Optional ancestor construct that owns this symbol namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ScopeSpec>,
}

/// Identifies a named ancestor construct as the owner of a symbol namespace.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeSpec {
    /// Construct ID that establishes the scope.
    pub construct: String,
    /// Identifier field whose value distinguishes this scope from other instances.
    pub name_field: String,
    /// Whether references search only this scope or continue through enclosing
    /// matching scopes when no declaration exists here.
    #[serde(default, skip_serializing_if = "ScopeLookup::is_exact")]
    pub lookup: ScopeLookup,
}

/// Name-resolution policy for a scoped reference.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeLookup {
    /// Resolve only in the nearest matching scope.
    #[default]
    Exact,
    /// Search matching enclosing scopes from nearest to outermost.
    LexicalParents,
}

impl ScopeLookup {
    const fn is_exact(lookup: &Self) -> bool {
        matches!(lookup, Self::Exact)
    }
}
