//! Canonical, serializable language-definition data types.

use serde::{Deserialize, Deserializer, Serialize};

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
    /// Optional built-in lexical rules. When absent, Langmeld keeps the legacy
    /// hash line-comment behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lexical: Option<LexicalSpec>,
    /// Optional generic name-resolution policies keyed by declared symbol kind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbol_policies: Vec<SymbolPolicySpec>,
    /// Reusable kinds that constructs may implement and blocks may accept.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<CategorySpec>,
    /// Named syntax fragments expanded into construct patterns during validation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<SyntaxRuleSpec>,
    /// Reusable enumerations that fields can reference by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enums: Vec<EnumSpec>,
    /// Reusable operator grammars referenced by expression fields.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expressions: Vec<ExpressionSpec>,
    /// Named record schemas that definitions can reuse as field types.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<TypeSpec>,
    /// Host behavior stages required by this language.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub behaviors: Vec<BehaviorSpec>,
    /// Constructs available in this language.
    pub constructs: Vec<ConstructSpec>,
}

/// Comment delimiters recognized by Langmeld's built-in lexer.
///
/// An absent lexical section keeps the legacy `#` line comment. An authored
/// section replaces that default; `None` explicitly disables line comments.
/// Block comments are non-nested. Languages that need nesting or other lexical
/// state can register a custom lexer through the language crate.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LexicalSpec {
    /// Prefix that begins a line comment, or `None` to disable line comments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_comment: Option<String>,
    /// Non-nested block-comment delimiters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_comment: Option<BlockCommentSpec>,
}

impl LexicalSpec {
    /// Creates an explicit lexical policy with no comment delimiters.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            line_comment: None,
            block_comment: None,
        }
    }

    /// Sets the line-comment prefix.
    #[must_use]
    pub fn line_comment(mut self, marker: impl Into<String>) -> Self {
        self.line_comment = Some(marker.into());
        self
    }

    /// Disables line comments in this explicit policy.
    #[must_use]
    pub fn without_line_comment(mut self) -> Self {
        self.line_comment = None;
        self
    }

    /// Sets the non-nested block-comment delimiters.
    #[must_use]
    pub fn block_comment(mut self, open: impl Into<String>, close: impl Into<String>) -> Self {
        self.block_comment = Some(BlockCommentSpec {
            open: open.into(),
            close: close.into(),
        });
        self
    }
}

impl Default for LexicalSpec {
    fn default() -> Self {
        Self {
            line_comment: Some("#".to_owned()),
            block_comment: None,
        }
    }
}

/// Opening and closing source delimiters for one non-nested block comment.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BlockCommentSpec {
    /// Source sequence that opens a block comment.
    pub open: String,
    /// Source sequence that closes a block comment.
    pub close: String,
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

/// A reusable operator table for expression-valued fields.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpressionSpec {
    /// Stable identifier referenced by expression fields.
    pub id: String,
    /// Built-in atom forms accepted by this grammar, in no semantic order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub atoms: Vec<ExpressionAtomKind>,
    /// Optional host parser registration for additional atom forms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_atom_parser: Option<String>,
    /// Operators accepted by this grammar.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operators: Vec<ExpressionOperatorSpec>,
}

/// A built-in atom category available in a declarative expression grammar.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpressionAtomKind {
    /// Decimal numeric literals.
    Number,
    /// The `true` and `false` literals.
    Boolean,
    /// Identifier names.
    Identifier,
}

/// An operator spelling with one or both supported fixities.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpressionOperatorSpec {
    /// Punctuation spelling, such as `+` or `**`.
    pub token: String,
    /// Prefix binding power, when this operator has prefix fixity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix_precedence: Option<u8>,
    /// Infix binding power and associativity, when this operator has infix fixity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub infix: Option<InfixOperatorSpec>,
}

/// Infix binding behavior for one declared operator spelling.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InfixOperatorSpec {
    /// Binding power used by the Pratt parser.
    pub precedence: u8,
    /// How equal-precedence operators group.
    pub associativity: ExpressionAssociativity,
}

/// Associativity for an infix expression operator.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpressionAssociativity {
    /// Operators of the same precedence group from left to right.
    Left,
    /// Operators of the same precedence group from right to left.
    Right,
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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

// Serde cannot pair `flatten` with `deny_unknown_fields`, so validate the
// flattened kind keys explicitly while preserving the public wire shape.
impl<'de> Deserialize<'de> for FieldSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        const FIELDS: &[&str] = &[
            "name",
            "kind",
            "required",
            "default",
            "constraints",
            "values",
            "enum_name",
            "target",
            "record",
            "fields",
            "item",
            "key",
            "value",
        ];

        struct FieldSpecVisitor;

        impl<'de> Visitor<'de> for FieldSpecVisitor {
            type Value = FieldSpec;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a flattened field specification")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut name = None;
                let mut required = None;
                let mut default = None;
                let mut constraints = None;
                let mut kind_fields = FieldKindFields::default();

                while let Some(field) = map.next_key::<String>()? {
                    macro_rules! read_once {
                        ($slot:ident, $field:literal, $ty:ty) => {{
                            if $slot.is_some() {
                                return Err(de::Error::duplicate_field($field));
                            }
                            $slot = Some(map.next_value::<$ty>()?);
                        }};
                    }

                    match field.as_str() {
                        "name" => read_once!(name, "name", String),
                        "kind" => kind_fields.read("kind", &mut map, FIELDS)?,
                        "required" => read_once!(required, "required", bool),
                        "default" => read_once!(default, "default", Option<String>),
                        "constraints" => {
                            read_once!(constraints, "constraints", FieldConstraints)
                        }
                        _ => kind_fields.read(&field, &mut map, FIELDS)?,
                    }
                }

                let kind = kind_fields.build()?;

                Ok(FieldSpec {
                    name: name.ok_or_else(|| de::Error::missing_field("name"))?,
                    kind,
                    required: required.unwrap_or_else(default_true),
                    default: default.unwrap_or(None),
                    constraints: constraints.unwrap_or_default(),
                })
            }
        }

        deserializer.deserialize_struct("FieldSpec", FIELDS, FieldSpecVisitor)
    }
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
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
    /// A delimited record of named fields, usually shared through an `.langmeld` properties bundle.
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
    /// A source expression parsed with a declared operator table to a boundary token.
    Expression {
        /// Stable ID of the expression grammar declaration.
        grammar: String,
        /// Required punctuation token ending this expression capture.
        terminator: String,
    },
}

#[derive(Default)]
struct FieldKindFields {
    kind: Option<String>,
    values: Option<Vec<String>>,
    enum_name: Option<String>,
    target: Option<String>,
    record: Option<Option<String>>,
    fields: Option<Vec<FieldSpec>>,
    item: Option<Box<FieldKind>>,
    key: Option<MapKeyKind>,
    value: Option<Box<FieldKind>>,
    grammar: Option<String>,
    terminator: Option<String>,
}

impl FieldKindFields {
    fn read<'de, A>(
        &mut self,
        field: &str,
        map: &mut A,
        expected: &'static [&'static str],
    ) -> Result<(), A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        use serde::de::Error;

        macro_rules! read_once {
            ($slot:ident, $field:literal, $ty:ty) => {{
                if self.$slot.is_some() {
                    return Err(A::Error::duplicate_field($field));
                }
                self.$slot = Some(map.next_value::<$ty>()?);
            }};
        }

        match field {
            "kind" => read_once!(kind, "kind", String),
            "values" => read_once!(values, "values", Vec<String>),
            "enum_name" => read_once!(enum_name, "enum_name", String),
            "target" => read_once!(target, "target", String),
            "record" => read_once!(record, "record", Option<String>),
            "fields" => read_once!(fields, "fields", Vec<FieldSpec>),
            "item" => read_once!(item, "item", Box<FieldKind>),
            "key" => read_once!(key, "key", MapKeyKind),
            "value" => read_once!(value, "value", Box<FieldKind>),
            "grammar" => read_once!(grammar, "grammar", String),
            "terminator" => read_once!(terminator, "terminator", String),
            _ => return Err(A::Error::unknown_field(field, expected)),
        }
        Ok(())
    }

    fn build<E>(self) -> Result<FieldKind, E>
    where
        E: serde::de::Error,
    {
        const VARIANTS: &[&str] = &[
            "identifier",
            "number",
            "text",
            "enum",
            "named_enum",
            "reference",
            "properties",
            "list",
            "map",
            "expression",
        ];

        let kind_name = self.kind.ok_or_else(|| E::missing_field("kind"))?;
        let variant_fields = match kind_name.as_str() {
            "identifier" | "number" | "text" => &["kind"][..],
            "enum" => &["kind", "values"],
            "named_enum" => &["kind", "enum_name", "values"],
            "reference" => &["kind", "target"],
            "properties" => &["kind", "record", "fields"],
            "list" => &["kind", "item"],
            "map" => &["kind", "key", "value"],
            "expression" => &["kind", "grammar", "terminator"],
            _ => return Err(E::unknown_variant(&kind_name, VARIANTS)),
        };

        for (field, present) in [
            ("values", self.values.is_some()),
            ("enum_name", self.enum_name.is_some()),
            ("target", self.target.is_some()),
            ("record", self.record.is_some()),
            ("fields", self.fields.is_some()),
            ("item", self.item.is_some()),
            ("key", self.key.is_some()),
            ("value", self.value.is_some()),
            ("grammar", self.grammar.is_some()),
            ("terminator", self.terminator.is_some()),
        ] {
            if present && !variant_fields.contains(&field) {
                return Err(E::custom(format!(
                    "field `{field}` is not valid for `{kind_name}`"
                )));
            }
        }

        match kind_name.as_str() {
            "identifier" => Ok(FieldKind::Identifier),
            "number" => Ok(FieldKind::Number),
            "text" => Ok(FieldKind::Text),
            "enum" => Ok(FieldKind::Enum {
                values: self.values.ok_or_else(|| E::missing_field("values"))?,
            }),
            "named_enum" => Ok(FieldKind::NamedEnum {
                name: self
                    .enum_name
                    .ok_or_else(|| E::missing_field("enum_name"))?,
                values: self.values.ok_or_else(|| E::missing_field("values"))?,
            }),
            "reference" => Ok(FieldKind::Reference {
                target: self.target.ok_or_else(|| E::missing_field("target"))?,
            }),
            "properties" => Ok(FieldKind::Properties {
                name: self.record.unwrap_or(None),
                fields: self.fields.ok_or_else(|| E::missing_field("fields"))?,
            }),
            "list" => Ok(FieldKind::List {
                item: self.item.ok_or_else(|| E::missing_field("item"))?,
            }),
            "map" => Ok(FieldKind::Map {
                key: self.key.ok_or_else(|| E::missing_field("key"))?,
                value: self.value.ok_or_else(|| E::missing_field("value"))?,
            }),
            "expression" => Ok(FieldKind::Expression {
                grammar: self.grammar.ok_or_else(|| E::missing_field("grammar"))?,
                terminator: self
                    .terminator
                    .ok_or_else(|| E::missing_field("terminator"))?,
            }),
            _ => Err(E::unknown_variant(&kind_name, VARIANTS)),
        }
    }
}

impl<'de> Deserialize<'de> for FieldKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{MapAccess, Visitor};
        use std::fmt;

        const FIELDS: &[&str] = &[
            "kind",
            "values",
            "enum_name",
            "target",
            "record",
            "fields",
            "item",
            "key",
            "value",
            "grammar",
            "terminator",
        ];

        struct FieldKindVisitor;

        impl<'de> Visitor<'de> for FieldKindVisitor {
            type Value = FieldKind;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a field kind")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut fields = FieldKindFields::default();
                while let Some(field) = map.next_key::<String>()? {
                    fields.read(&field, &mut map, FIELDS)?;
                }
                fields.build()
            }
        }

        deserializer.deserialize_struct("FieldKind", FIELDS, FieldKindVisitor)
    }
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

/// A source-token separator recognized between constructs in a block.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum BlockSeparator {
    /// A line break token.
    Newline,
    /// A literal token sequence, such as `;` or `,`.
    Literal(String),
}

impl BlockSeparator {
    /// Creates a literal token-sequence separator.
    #[must_use]
    pub fn literal(value: impl Into<String>) -> Self {
        Self::Literal(value.into())
    }
}

/// Whether configured block separators must occur between adjacent children.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockSeparatorPolicy {
    /// Consume configured separators when present, but allow adjacent children.
    #[default]
    Optional,
    /// Require at least one configured separator between adjacent children.
    Required,
}

/// Child construct kinds and layout permitted in a construct body.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BlockSpec {
    /// IDs of constructs accepted in this body.
    pub allowed: Vec<String>,
    /// Literal token sequence that opens this block.
    #[serde(
        default = "default_block_open",
        skip_serializing_if = "is_default_block_open"
    )]
    pub open: String,
    /// Literal token sequence that closes this block.
    #[serde(
        default = "default_block_close",
        skip_serializing_if = "is_default_block_close"
    )]
    pub close: String,
    /// Separators recognized between block children.
    #[serde(
        default = "default_block_separators",
        skip_serializing_if = "is_default_block_separators"
    )]
    pub separators: Vec<BlockSeparator>,
    /// Whether separators are optional or required between adjacent children.
    #[serde(default, skip_serializing_if = "is_optional_block_separator_policy")]
    pub separator_policy: BlockSeparatorPolicy,
}

impl Default for BlockSpec {
    fn default() -> Self {
        Self {
            allowed: Vec::new(),
            open: default_block_open(),
            close: default_block_close(),
            separators: default_block_separators(),
            separator_policy: BlockSeparatorPolicy::Optional,
        }
    }
}

impl BlockSpec {
    /// Returns whether this block uses the legacy brace and separator defaults.
    #[must_use]
    pub fn uses_default_layout(&self) -> bool {
        self.open == default_block_open()
            && self.close == default_block_close()
            && self.separators == default_block_separators()
            && self.separator_policy == BlockSeparatorPolicy::Optional
    }
}

fn default_block_open() -> String {
    "{".to_owned()
}

fn default_block_close() -> String {
    "}".to_owned()
}

fn default_block_separators() -> Vec<BlockSeparator> {
    vec![BlockSeparator::Newline, BlockSeparator::literal(";")]
}

fn is_default_block_open(value: &String) -> bool {
    value == "{"
}

fn is_default_block_close(value: &String) -> bool {
    value == "}"
}

fn is_default_block_separators(value: &[BlockSeparator]) -> bool {
    value == default_block_separators()
}

fn is_optional_block_separator_policy(value: &BlockSeparatorPolicy) -> bool {
    *value == BlockSeparatorPolicy::Optional
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

/// Generic name-resolution rules for one declared symbol kind.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolPolicySpec {
    /// Symbol kind this policy controls.
    pub kind: String,
    /// Whether references can see declarations later in the same source file.
    #[serde(default, skip_serializing_if = "DeclarationOrder::is_hoisted")]
    pub declaration_order: DeclarationOrder,
    /// How repeated names in the same scope are handled.
    #[serde(default, skip_serializing_if = "DuplicatePolicy::is_error")]
    pub duplicates: DuplicatePolicy,
    /// Whether a declaration may reuse a name from an enclosing scope.
    #[serde(default, skip_serializing_if = "ShadowingPolicy::is_allowed")]
    pub shadowing: ShadowingPolicy,
}

/// Declaration visibility order within one source file.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeclarationOrder {
    /// Declarations are visible throughout their scope.
    #[default]
    Hoisted,
    /// A declaration is visible only after its source position.
    Sequential,
}

impl DeclarationOrder {
    const fn is_hoisted(value: &Self) -> bool {
        matches!(value, Self::Hoisted)
    }
}

/// Policy for repeated names in the same scope.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicatePolicy {
    /// Report a duplicate and keep the first declaration as the reference target.
    #[default]
    Error,
    /// Permit duplicate declarations, reporting ambiguity when a reference matches them.
    AllowAmbiguous,
}

impl DuplicatePolicy {
    const fn is_error(value: &Self) -> bool {
        matches!(value, Self::Error)
    }
}

/// Policy for reusing a name from an enclosing lexical scope.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShadowingPolicy {
    /// Inner declarations may reuse names from enclosing scopes.
    #[default]
    Allow,
    /// Diagnose inner declarations that reuse a visible enclosing name.
    Disallow,
}

impl ShadowingPolicy {
    const fn is_allowed(value: &Self) -> bool {
        matches!(value, Self::Allow)
    }
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

/// A language definition that passed structural validation.
#[derive(Clone, Debug)]
pub struct ValidatedLanguageSpec(pub(super) LanguageSpec, pub(super) Vec<SpecWarning>);

impl ValidatedLanguageSpec {
    /// Returns the immutable canonical definition.
    #[must_use]
    pub const fn spec(&self) -> &LanguageSpec {
        &self.0
    }

    /// Finds a construct by its stable identifier.
    #[must_use]
    pub fn construct(&self, id: &str) -> Option<&ConstructSpec> {
        self.0
            .constructs
            .iter()
            .find(|construct| construct.id == id)
    }

    /// Returns non-fatal findings produced while validating this definition.
    #[must_use]
    pub fn warnings(&self) -> &[SpecWarning] {
        &self.1
    }
}

/// A non-fatal finding about a language definition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpecWarning {
    /// Stable validation code.
    pub code: &'static str,
    /// Explanation of the condition that limited validation.
    pub message: String,
}

/// A problem found while validating a language definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecError {
    /// Stable validation code.
    pub code: &'static str,
    /// Explanation of the invalid declaration.
    pub message: String,
}
