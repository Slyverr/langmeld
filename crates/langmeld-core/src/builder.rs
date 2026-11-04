//! Fluent Rust authoring APIs for language specifications.

use std::fmt;

use super::*;

impl FieldConstraints {
    /// Sets inclusive numeric bounds for a number field.
    #[must_use]
    pub const fn numeric_range(min: Option<i64>, max: Option<i64>) -> Self {
        Self {
            min,
            max,
            ..Self::empty()
        }
    }

    /// Sets minimum and maximum Unicode scalar counts for a text field.
    #[must_use]
    pub const fn text_length(min_length: Option<usize>, max_length: Option<usize>) -> Self {
        Self {
            min_length,
            max_length,
            ..Self::empty()
        }
    }

    /// Sets minimum and maximum entry counts for a collection or property record.
    #[must_use]
    pub const fn item_count(min_items: Option<usize>, max_items: Option<usize>) -> Self {
        Self {
            min_items,
            max_items,
            ..Self::empty()
        }
    }

    const fn empty() -> Self {
        Self {
            min: None,
            max: None,
            min_length: None,
            max_length: None,
            min_items: None,
            max_items: None,
        }
    }
}

impl FieldSpec {
    /// Declares a required field with the given type.
    #[must_use]
    pub fn required(name: impl Into<String>, kind: FieldKind) -> Self {
        Self {
            name: name.into(),
            kind,
            required: true,
            default: None,
            constraints: FieldConstraints::default(),
        }
    }

    /// Declares an optional field with no default.
    #[must_use]
    pub fn optional(name: impl Into<String>, kind: FieldKind) -> Self {
        Self {
            name: name.into(),
            kind,
            required: false,
            default: None,
            constraints: FieldConstraints::default(),
        }
    }

    /// Declares a required field whose value is supplied when syntax omits it.
    #[must_use]
    pub fn with_default(
        name: impl Into<String>,
        kind: FieldKind,
        value: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            kind,
            required: true,
            default: Some(value.into()),
            constraints: FieldConstraints::default(),
        }
    }

    /// Sets the value inserted when source syntax omits this field, preserving its requiredness.
    #[must_use]
    pub fn default_value(mut self, value: impl Into<String>) -> Self {
        self.default = Some(value.into());
        self
    }

    /// Attaches numeric, text-length, or collection-size constraints.
    #[must_use]
    pub fn constraints(mut self, constraints: FieldConstraints) -> Self {
        self.constraints = constraints;
        self
    }
}

/// A composable source-syntax template accepted by Langmeld's pattern parser.
///
/// This offers a structured Rust authoring path while rendering to the canonical pattern
/// string used by `.langmeld` definitions. Use [`Self::template`] for an existing pattern string.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyntaxPattern(String);

impl SyntaxPattern {
    /// Creates a literal token sequence, escaping characters reserved by the pattern syntax.
    #[must_use]
    pub fn literal(value: impl AsRef<str>) -> Self {
        let mut pattern = String::new();
        for character in value.as_ref().chars() {
            if matches!(
                character,
                '[' | ']' | '(' | ')' | '|' | '{' | '}' | '\\' | '@'
            ) {
                pattern.push('\\');
            }
            pattern.push(character);
        }
        Self(pattern)
    }

    /// Captures one syntax value into a construct field.
    #[must_use]
    pub fn capture(field: impl AsRef<str>) -> Self {
        Self(format!("{{{}}}", field.as_ref()))
    }

    /// Uses an existing Langmeld syntax template as a composable fragment.
    #[must_use]
    pub fn template(pattern: impl Into<String>) -> Self {
        Self(pattern.into())
    }

    /// Sequences this fragment before another fragment.
    #[must_use]
    pub fn then(self, next: impl Into<Self>) -> Self {
        let next = next.into();
        match (self.0.is_empty(), next.0.is_empty()) {
            (true, _) => next,
            (_, true) => self,
            (false, false) => Self(format!("{} {}", self.0, next.0)),
        }
    }

    /// Makes this fragment optional.
    #[must_use]
    pub fn optional(self) -> Self {
        Self(format!("[{}]", self.0))
    }

    /// Matches this fragment zero or more times.
    #[must_use]
    pub fn zero_or_more(self) -> Self {
        Self(format!("({})*", self.0))
    }

    /// Matches this fragment one or more times.
    #[must_use]
    pub fn one_or_more(self) -> Self {
        Self(format!("({})+", self.0))
    }

    /// Matches one of several alternative fragments.
    #[must_use]
    pub fn choice<I, P>(alternatives: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<Self>,
    {
        Self(format!(
            "({})",
            alternatives
                .into_iter()
                .map(|alternative| alternative.into().0)
                .collect::<Vec<_>>()
                .join("|")
        ))
    }

    /// Matches this fragment as a comma-separated list, allowing a trailing comma.
    #[must_use]
    pub fn comma_separated(self) -> Self {
        self.separated_by(",")
    }

    /// Matches this fragment as a separated list, allowing a trailing separator.
    ///
    /// `separator` is treated as literal source text and may contain multiple
    /// tokens. For custom parsing behavior, register a custom construct parser.
    #[must_use]
    pub fn separated_by(self, separator: impl AsRef<str>) -> Self {
        let separator = Self::literal(separator);
        Self(format!("({}[{}])*", self.0, separator.0))
    }

    /// Returns the canonical Langmeld pattern string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for SyntaxPattern {
    fn from(pattern: &str) -> Self {
        Self::template(pattern)
    }
}

impl From<String> for SyntaxPattern {
    fn from(pattern: String) -> Self {
        Self::template(pattern)
    }
}

impl fmt::Display for SyntaxPattern {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Fluent builder for a construct in a language definition.
#[derive(Clone, Debug)]
pub struct ConstructBuilder(ConstructSpec);

impl ConstructBuilder {
    /// Starts a construct definition with a stable identifier.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(ConstructSpec {
            id: id.into(),
            implements: Vec::new(),
            custom_parser: None,
            syntax: Vec::new(),
            syntax_ids: Vec::new(),
            fields: Vec::new(),
            block: None,
            symbol: None,
        })
    }

    /// Adds a syntax template. Call once per accepted variant.
    #[must_use]
    pub fn syntax(mut self, pattern: impl Into<String>) -> Self {
        self.0.syntax.push(pattern.into());
        self
    }

    /// Adds unnamed syntax templates in iterator order.
    #[must_use]
    pub fn syntax_templates<I, Pattern>(mut self, patterns: I) -> Self
    where
        I: IntoIterator<Item = Pattern>,
        Pattern: Into<String>,
    {
        self.0.syntax.extend(patterns.into_iter().map(Into::into));
        self
    }

    /// Adds a syntax template assembled from [`SyntaxPattern`] parts.
    #[must_use]
    pub fn syntax_pattern(self, pattern: impl Into<SyntaxPattern>) -> Self {
        self.syntax(pattern.into().0)
    }

    /// Adds a syntax template with its stable variant identifier as one operation.
    ///
    /// Use [`Self::syntax`] for unnamed variants. A construct's variants must be
    /// consistently named or unnamed, matching the declarative `.langmeld` frontend.
    #[must_use]
    pub fn syntax_variant(mut self, id: impl Into<String>, pattern: impl Into<String>) -> Self {
        self.0.syntax.push(pattern.into());
        self.0.syntax_ids.push(id.into());
        self
    }

    /// Adds named syntax variants in iterator order.
    #[must_use]
    pub fn syntax_variants<I, Id, Pattern>(mut self, variants: I) -> Self
    where
        I: IntoIterator<Item = (Id, Pattern)>,
        Id: Into<String>,
        Pattern: Into<String>,
    {
        for (id, pattern) in variants {
            self = self.syntax_variant(id, pattern);
        }
        self
    }

    /// Adds a named syntax variant assembled from [`SyntaxPattern`] parts.
    #[must_use]
    pub fn syntax_variant_pattern(
        self,
        id: impl Into<String>,
        pattern: impl Into<SyntaxPattern>,
    ) -> Self {
        self.syntax_variant(id, pattern.into().0)
    }

    /// Adds an explicit stable identifier for the most recently added syntax template.
    #[must_use]
    pub fn syntax_id(mut self, id: impl Into<String>) -> Self {
        self.0.syntax_ids.push(id.into());
        self
    }

    /// Adds reusable categories implemented by this construct.
    ///
    /// Repeated calls accumulate categories, so a construct can be composed in
    /// several builder steps.
    #[must_use]
    pub fn implements<I, S>(mut self, categories: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.0
            .implements
            .extend(categories.into_iter().map(Into::into));
        self
    }

    /// Adds one reusable category implemented by this construct.
    #[must_use]
    pub fn implement(mut self, category: impl Into<String>) -> Self {
        self.0.implements.push(category.into());
        self
    }

    /// Adds a captured field.
    #[must_use]
    pub fn field(mut self, field: FieldSpec) -> Self {
        self.0.fields.push(field);
        self
    }

    /// Adds captured fields in iterator order.
    #[must_use]
    pub fn fields<I>(mut self, fields: I) -> Self
    where
        I: IntoIterator<Item = FieldSpec>,
    {
        self.0.fields.extend(fields);
        self
    }

    /// Restricts the child constructs accepted by this construct's block.
    #[must_use]
    pub fn block<I, S>(mut self, allowed: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.block_spec_mut().allowed = allowed.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the literal token sequences that open and close this block.
    #[must_use]
    pub fn block_delimiters(mut self, open: impl Into<String>, close: impl Into<String>) -> Self {
        let block = self.block_spec_mut();
        block.open = open.into();
        block.close = close.into();
        self
    }

    /// Sets the separators recognized between block children and their policy.
    #[must_use]
    pub fn block_separators(
        mut self,
        separators: impl IntoIterator<Item = BlockSeparator>,
        policy: BlockSeparatorPolicy,
    ) -> Self {
        let block = self.block_spec_mut();
        block.separators = separators.into_iter().collect();
        block.separator_policy = policy;
        self
    }

    fn block_spec_mut(&mut self) -> &mut BlockSpec {
        self.0.block.get_or_insert_with(BlockSpec::default)
    }

    /// Adds generic symbol declaration metadata.
    #[must_use]
    pub fn symbol(mut self, symbol: SymbolSpec) -> Self {
        self.0.symbol = Some(symbol);
        self
    }

    /// Selects a host-provided parser callback for this construct.
    #[must_use]
    pub fn custom_parser(mut self, parser: impl Into<String>) -> Self {
        self.0.custom_parser = Some(parser.into());
        self
    }
}

impl From<ConstructBuilder> for ConstructSpec {
    fn from(builder: ConstructBuilder) -> Self {
        builder.0
    }
}

/// Fluent authoring frontend for the canonical language specification.
#[derive(Clone, Debug)]
pub struct LanguageBuilder {
    spec: LanguageSpec,
}

impl LanguageBuilder {
    /// Starts a language definition. The schema version defaults to 1.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            spec: LanguageSpec {
                schema_version: 1,
                name: name.into(),
                version: String::new(),
                root: String::new(),
                source_extensions: Vec::new(),
                categories: Vec::new(),
                rules: Vec::new(),
                enums: Vec::new(),
                expressions: Vec::new(),
                types: Vec::new(),
                behaviors: Vec::new(),
                constructs: Vec::new(),
            },
        }
    }

    /// Selects the canonical definition schema version.
    #[must_use]
    pub fn schema_version(mut self, schema_version: u32) -> Self {
        self.spec.schema_version = schema_version;
        self
    }

    /// Selects the language-definition version.
    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.spec.version = version.into();
        self
    }

    /// Selects the required root construct.
    #[must_use]
    pub fn root(mut self, construct: impl Into<String>) -> Self {
        self.spec.root = construct.into();
        self
    }

    /// Adds a source filename suffix, such as `.calc` or `.d.ts`.
    #[must_use]
    pub fn source_extension(mut self, extension: impl Into<String>) -> Self {
        self.spec.source_extensions.push(extension.into());
        self
    }

    /// Adds an ordered group of source filename suffixes.
    #[must_use]
    pub fn source_extensions<I, S>(mut self, extensions: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.spec
            .source_extensions
            .extend(extensions.into_iter().map(Into::into));
        self
    }

    /// Adds a construct declaration.
    #[must_use]
    pub fn construct(mut self, construct: impl Into<ConstructSpec>) -> Self {
        self.spec.constructs.push(construct.into());
        self
    }

    /// Adds an ordered group of construct declarations.
    ///
    /// This is useful when a language definition is composed from focused Rust
    /// modules. Repeated calls append constructs in iteration order, just like
    /// repeated calls to [`Self::construct`].
    #[must_use]
    pub fn constructs<I, C>(mut self, constructs: I) -> Self
    where
        I: IntoIterator<Item = C>,
        C: Into<ConstructSpec>,
    {
        self.spec
            .constructs
            .extend(constructs.into_iter().map(Into::into));
        self
    }

    /// Declares a reusable construct category.
    #[must_use]
    pub fn category(mut self, id: impl Into<String>) -> Self {
        self.spec.categories.push(CategorySpec { id: id.into() });
        self
    }

    /// Adds an ordered group of reusable construct categories.
    #[must_use]
    pub fn categories<I, S>(mut self, categories: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.spec.categories.extend(
            categories
                .into_iter()
                .map(|id| CategorySpec { id: id.into() }),
        );
        self
    }

    /// Declares a reusable syntax fragment.
    #[must_use]
    pub fn rule(mut self, id: impl Into<String>, pattern: impl Into<String>) -> Self {
        self.spec.rules.push(SyntaxRuleSpec {
            id: id.into(),
            parameters: Vec::new(),
            pattern: pattern.into(),
        });
        self
    }

    /// Declares a reusable syntax fragment with captures mapped at each use.
    #[must_use]
    pub fn parameterized_rule(
        mut self,
        id: impl Into<String>,
        parameters: impl IntoIterator<Item = impl Into<String>>,
        pattern: impl Into<String>,
    ) -> Self {
        self.spec.rules.push(SyntaxRuleSpec {
            id: id.into(),
            parameters: parameters.into_iter().map(Into::into).collect(),
            pattern: pattern.into(),
        });
        self
    }

    /// Adds an ordered group of reusable syntax fragments.
    #[must_use]
    pub fn rules<I, R>(mut self, rules: I) -> Self
    where
        I: IntoIterator<Item = R>,
        R: Into<SyntaxRuleSpec>,
    {
        self.spec.rules.extend(rules.into_iter().map(Into::into));
        self
    }

    /// Declares a reusable named record type for structured field values.
    #[must_use]
    pub fn named_type(
        self,
        id: impl Into<String>,
        fields: impl IntoIterator<Item = FieldSpec>,
    ) -> Self {
        self.record_type(NamedRecord::new(id, fields))
    }

    /// Declares a reusable named enumeration.
    #[must_use]
    pub fn named_enum(
        self,
        id: impl Into<String>,
        values: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.enum_type(NamedEnum::new(id, values))
    }

    /// Adds a group of reusable named enumerations in iterator order.
    ///
    /// [`Self::build`] sorts enum declarations by ID in the validated canonical
    /// specification because declaration order has no parsing semantics.
    #[must_use]
    pub fn named_enums<I, E>(mut self, enumerations: I) -> Self
    where
        I: IntoIterator<Item = E>,
        E: Into<NamedEnum>,
    {
        self.spec
            .enums
            .extend(enumerations.into_iter().map(|item| item.into().spec));
        self
    }

    /// Registers a reusable enumeration declaration.
    #[must_use]
    pub fn enum_type(mut self, enumeration: NamedEnum) -> Self {
        self.spec.enums.push(enumeration.spec);
        self
    }

    /// Registers a reusable expression operator grammar.
    #[must_use]
    pub fn expression(mut self, expression: ExpressionSpec) -> Self {
        self.spec.expressions.push(expression);
        self
    }

    /// Adds ordered expression operator grammars.
    #[must_use]
    pub fn expressions<I, E>(mut self, expressions: I) -> Self
    where
        I: IntoIterator<Item = E>,
        E: Into<ExpressionSpec>,
    {
        self.spec
            .expressions
            .extend(expressions.into_iter().map(Into::into));
        self
    }

    /// Registers a reusable record declaration created with [`NamedRecord`].
    #[must_use]
    pub fn record_type(mut self, record: NamedRecord) -> Self {
        self.spec.types.push(record.spec);
        self
    }

    /// Adds a group of reusable named record types in iterator order.
    ///
    /// [`Self::build`] sorts record declarations by ID in the validated canonical
    /// specification because declaration order has no parsing semantics.
    #[must_use]
    pub fn record_types<I, R>(mut self, records: I) -> Self
    where
        I: IntoIterator<Item = R>,
        R: Into<NamedRecord>,
    {
        self.spec
            .types
            .extend(records.into_iter().map(|item| item.into().spec));
        self
    }

    /// Declares a host-provided behavior required at a specific analysis stage.
    #[must_use]
    pub fn behavior(mut self, id: impl Into<String>, stage: BehaviorStage) -> Self {
        self.spec.behaviors.push(BehaviorSpec {
            id: id.into(),
            stage,
        });
        self
    }

    /// Adds an ordered group of host-provided behavior requirements.
    #[must_use]
    pub fn behaviors<I, B>(mut self, behaviors: I) -> Self
    where
        I: IntoIterator<Item = B>,
        B: Into<BehaviorSpec>,
    {
        self.spec
            .behaviors
            .extend(behaviors.into_iter().map(Into::into));
        self
    }

    /// Validates and returns the canonical language definition.
    ///
    /// # Errors
    /// Returns all structural errors found in the assembled definition.
    pub fn build(self) -> Result<ValidatedLanguageSpec, Vec<SpecError>> {
        self.into_spec().validate()
    }

    /// Consumes the builder and returns the assembled specification without validation.
    ///
    /// Use [`Self::build`] when the language should be validated immediately. This is useful
    /// when a caller needs to compose or transform specifications before validating them.
    #[must_use]
    pub fn into_spec(self) -> LanguageSpec {
        self.spec
    }
}

impl FieldKind {
    /// Captures an expression using a named grammar and explicit punctuation terminator.
    #[must_use]
    pub fn expression(grammar: impl Into<String>, terminator: impl Into<String>) -> Self {
        Self::Expression {
            grammar: grammar.into(),
            terminator: terminator.into(),
        }
    }
    /// Constrains an identifier to one of the declared values.
    #[must_use]
    pub fn enumeration(values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self::Enum {
            values: values.into_iter().map(Into::into).collect(),
        }
    }

    /// References a reusable named enumeration while preserving its identity.
    #[must_use]
    pub fn named_enumeration(
        name: impl Into<String>,
        values: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self::NamedEnum {
            name: name.into(),
            values: values.into_iter().map(Into::into).collect(),
        }
    }

    /// References declarations of the given symbol kind.
    #[must_use]
    pub fn reference(target: impl Into<String>) -> Self {
        Self::Reference {
            target: target.into(),
        }
    }

    /// Parses an inline record with the supplied fields.
    #[must_use]
    pub fn properties(fields: impl IntoIterator<Item = FieldSpec>) -> Self {
        Self::Properties {
            name: None,
            fields: fields.into_iter().collect(),
        }
    }

    /// Parses a list whose items use the supplied field kind.
    #[must_use]
    pub fn list(item: Self) -> Self {
        Self::List {
            item: Box::new(item),
        }
    }

    /// Parses a map with the supplied key and value kinds.
    #[must_use]
    pub fn map(key: MapKeyKind, value: Self) -> Self {
        Self::Map {
            key,
            value: Box::new(value),
        }
    }

    /// References a reusable named record while retaining its structural schema.
    #[must_use]
    pub fn named_record(
        id: impl Into<String>,
        fields: impl IntoIterator<Item = FieldSpec>,
    ) -> Self {
        Self::Properties {
            name: Some(id.into()),
            fields: fields.into_iter().collect(),
        }
    }
}

impl ExpressionSpec {
    /// Starts a reusable expression grammar with no atoms or operators.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            atoms: Vec::new(),
            host_atom_parser: None,
            operators: Vec::new(),
        }
    }

    /// Enables one built-in expression atom category.
    #[must_use]
    pub fn atom(mut self, atom: ExpressionAtomKind) -> Self {
        self.atoms.push(atom);
        self
    }

    /// Requires a host callback for syntax forms outside the built-in atoms.
    #[must_use]
    pub fn host_atom_parser(mut self, id: impl Into<String>) -> Self {
        self.host_atom_parser = Some(id.into());
        self
    }

    /// Adds an expression operator declaration.
    #[must_use]
    pub fn operator(mut self, operator: ExpressionOperatorSpec) -> Self {
        self.operators.push(operator);
        self
    }
}

impl ExpressionOperatorSpec {
    /// Declares a prefix operator.
    #[must_use]
    pub fn prefix(token: impl Into<String>, precedence: u8) -> Self {
        Self {
            token: token.into(),
            prefix_precedence: Some(precedence),
            infix: None,
        }
    }

    /// Declares an infix operator.
    #[must_use]
    pub fn infix(
        token: impl Into<String>,
        precedence: u8,
        associativity: ExpressionAssociativity,
    ) -> Self {
        Self {
            token: token.into(),
            prefix_precedence: None,
            infix: Some(InfixOperatorSpec {
                precedence,
                associativity,
            }),
        }
    }

    /// Declares one token as both prefix and infix.
    #[must_use]
    pub fn prefix_infix(
        token: impl Into<String>,
        prefix_precedence: u8,
        infix_precedence: u8,
        associativity: ExpressionAssociativity,
    ) -> Self {
        Self {
            token: token.into(),
            prefix_precedence: Some(prefix_precedence),
            infix: Some(InfixOperatorSpec {
                precedence: infix_precedence,
                associativity,
            }),
        }
    }
}

/// A named record declaration that can also be used as a field type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedRecord {
    spec: TypeSpec,
}

/// A named enumeration declaration that can also be used as a field kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedEnum {
    spec: EnumSpec,
}

impl NamedEnum {
    /// Declares a finite set of reusable identifier values.
    #[must_use]
    pub fn new(id: impl Into<String>, values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            spec: EnumSpec {
                id: id.into(),
                values: values.into_iter().map(Into::into).collect(),
            },
        }
    }

    /// Returns the stable enumeration identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.spec.id
    }

    /// Returns a field kind that retains this declaration's identity.
    #[must_use]
    pub fn field_kind(&self) -> FieldKind {
        FieldKind::named_enumeration(self.spec.id.clone(), self.spec.values.clone())
    }

    /// Registers this declaration with a language builder.
    #[must_use]
    pub fn register(self, language: LanguageBuilder) -> LanguageBuilder {
        language.enum_type(self)
    }
}

impl NamedRecord {
    /// Declares a reusable record schema.
    #[must_use]
    pub fn new(id: impl Into<String>, fields: impl IntoIterator<Item = FieldSpec>) -> Self {
        Self {
            spec: TypeSpec {
                id: id.into(),
                fields: fields.into_iter().collect(),
            },
        }
    }

    /// Returns the stable record identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.spec.id
    }

    /// Returns a field kind carrying this record's name and schema.
    #[must_use]
    pub fn field_kind(&self) -> FieldKind {
        FieldKind::named_record(self.spec.id.clone(), self.spec.fields.clone())
    }
}

impl SymbolSpec {
    /// Declares a symbol using the construct field that supplies its name.
    #[must_use]
    pub fn new(kind: impl Into<String>, name_field: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            name_field: name_field.into(),
            scope: None,
        }
    }

    /// Places this symbol class in an ancestor-owned namespace.
    #[must_use]
    pub fn with_scope(mut self, scope: ScopeSpec) -> Self {
        self.scope = Some(scope);
        self
    }
}

impl ScopeSpec {
    /// Creates a scope identified by a named ancestor construct and field.
    #[must_use]
    pub fn new(construct: impl Into<String>, name_field: impl Into<String>) -> Self {
        Self {
            construct: construct.into(),
            name_field: name_field.into(),
            lookup: ScopeLookup::Exact,
        }
    }

    /// Selects how references search this scope and its ancestors.
    #[must_use]
    pub const fn lookup(mut self, lookup: ScopeLookup) -> Self {
        self.lookup = lookup;
        self
    }
}
