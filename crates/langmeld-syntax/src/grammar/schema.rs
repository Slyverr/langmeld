//! Which nodes a tree has, and what they hold. It is described apart from parsing, so
//! that a hand-written parser can offer the same description as a grammar.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::{SyntaxKind, SyntaxNode};

/// How often a field occurs in a node of a well-formed source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Cardinality {
    /// Exactly once.
    One,
    /// Once or not at all.
    Optional,
    /// Any number of times.
    Many,
}

/// A named part of a node.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct FieldSchema {
    /// The label.
    pub name: String,
    /// The nodes or unions it can be.
    pub nodes: Vec<String>,
    /// The tokens it can be, by name or text.
    pub tokens: Vec<String>,
    /// How often it occurs.
    pub cardinality: Cardinality,
    /// Where it is among the children, or `None` if labels alone tell it apart from
    /// other parts, so that a tree cannot.
    pub place: Option<Place>,
}

/// Where a field is among the children of its node: the first ones of its kinds,
/// found between two tokens and after passing over some.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct Place {
    /// The field comes after the first of this token, such as `else`.
    pub after: Option<String>,
    /// How many children of its kinds come before it, there.
    pub skip: usize,
    /// The field comes before the first of this token that follows.
    pub before: Option<String>,
}

impl Place {
    /// After the first of the token `after`.
    pub fn after(mut self, after: impl Into<String>) -> Self {
        self.after = Some(after.into());
        self
    }

    /// Passing over `skip` children of the field's kinds.
    pub fn skip(mut self, skip: usize) -> Self {
        self.skip = skip;
        self
    }

    /// Before the first of the token `before`.
    pub fn before(mut self, before: impl Into<String>) -> Self {
        self.before = Some(before.into());
        self
    }
}

impl FieldSchema {
    /// A field that is one of the given nodes or unions.
    ///
    /// ```
    /// use langmeld_syntax::{Cardinality, FieldSchema};
    ///
    /// let field = FieldSchema::new("args", ["Expr"], Cardinality::Many);
    /// assert_eq!(field.nodes, ["Expr"]);
    /// ```
    pub fn new<S: Into<String>>(
        name: impl Into<String>,
        nodes: impl IntoIterator<Item = S>,
        cardinality: Cardinality,
    ) -> Self {
        Self {
            name: name.into(),
            nodes: nodes.into_iter().map(Into::into).collect(),
            tokens: Vec::new(),
            cardinality,
            place: Some(Place::default()),
        }
    }

    /// Says where the field is, when it is not the first child of its kinds.
    pub fn at(mut self, place: Place) -> Self {
        self.place = Some(place);
        self
    }

    /// A field that is one of the given tokens.
    ///
    /// ```
    /// use langmeld_syntax::{Cardinality, FieldSchema};
    ///
    /// let field = FieldSchema::token("operator", ["+", "-"], Cardinality::One);
    /// assert_eq!(field.tokens, ["+", "-"]);
    /// ```
    pub fn token<S: Into<String>>(
        name: impl Into<String>,
        tokens: impl IntoIterator<Item = S>,
        cardinality: Cardinality,
    ) -> Self {
        let mut field = Self::new(name, Vec::<String>::new(), cardinality);
        field.tokens = tokens.into_iter().map(Into::into).collect();
        field
    }
}

/// One kind of node.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct NodeSchema {
    /// The name of the node.
    pub name: String,
    /// Its labelled parts.
    pub fields: Vec<FieldSchema>,
    /// Every node or union that may be a direct child, labelled or not.
    pub children: BTreeSet<String>,
    /// Whether anything may be a child, as in nodes made by custom code.
    pub open: bool,
}

impl NodeSchema {
    /// A node with no children.
    ///
    /// ```
    /// use langmeld_syntax::NodeSchema;
    ///
    /// assert!(NodeSchema::new("Name").children.is_empty());
    /// ```
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            fields: Vec::new(),
            children: BTreeSet::new(),
            open: false,
        }
    }

    /// Adds a labelled part.
    ///
    /// ```
    /// use langmeld_syntax::{Cardinality, FieldSchema, NodeSchema};
    ///
    /// let node = NodeSchema::new("Let").with_field(FieldSchema::new(
    ///     "value",
    ///     ["Expr"],
    ///     Cardinality::One,
    /// ));
    /// assert!(node.field("value").is_some());
    /// assert!(node.children.contains("Expr"));
    /// ```
    pub fn with_field(mut self, field: FieldSchema) -> Self {
        self.children.extend(field.nodes.iter().cloned());
        self.fields.push(field);
        self
    }

    /// Allows a node or union as an unlabelled child.
    ///
    /// ```
    /// use langmeld_syntax::NodeSchema;
    ///
    /// let node = NodeSchema::new("Block").with_child("Stmt");
    /// assert!(node.children.contains("Stmt"));
    /// ```
    pub fn with_child(mut self, name: impl Into<String>) -> Self {
        self.children.insert(name.into());
        self
    }

    /// Allows any children.
    ///
    /// ```
    /// use langmeld_syntax::NodeSchema;
    ///
    /// assert!(NodeSchema::new("Custom").open().open);
    /// ```
    pub fn open(mut self) -> Self {
        self.open = true;
        self
    }

    /// The labelled part called `name`.
    ///
    /// ```
    /// use langmeld_syntax::{Cardinality, FieldSchema, NodeSchema};
    ///
    /// let node = NodeSchema::new("Call").with_field(FieldSchema::new(
    ///     "args",
    ///     ["Expr"],
    ///     Cardinality::Many,
    /// ));
    /// assert_eq!(node.field("args").map(|f| f.cardinality), Some(Cardinality::Many));
    /// assert!(node.field("callee").is_none());
    /// ```
    pub fn field(&self, name: &str) -> Option<&FieldSchema> {
        self.fields.iter().find(|field| field.name == name)
    }
}

/// Where a tree differs from its schema.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SchemaError {
    /// The node concerned.
    pub node: String,
    /// How it differs.
    pub message: String,
}

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` {}", self.node, self.message)
    }
}

/// The nodes of a language and the unions that group them.
///
/// A grammar provides its schema. For a parser of your own, describe the nodes and
/// check trees against them in tests:
///
/// ```
/// use langmeld_syntax::{Cardinality, FieldSchema, NodeSchema, Schema};
///
/// let schema = Schema::default()
///     .with_node(NodeSchema::new("Name"))
///     .with_node(NodeSchema::new("Let").with_field(FieldSchema::new(
///         "name",
///         ["Name"],
///         Cardinality::One,
///     )));
/// assert!(schema.node("Let").is_some());
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Schema {
    nodes: BTreeMap<String, NodeSchema>,
    unions: BTreeMap<String, Vec<String>>,
}

impl Schema {
    /// Adds a node, replacing one of the same name.
    ///
    /// ```
    /// use langmeld_syntax::{NodeSchema, Schema};
    ///
    /// let schema = Schema::default().with_node(NodeSchema::new("Name"));
    /// assert!(schema.node("Name").is_some());
    /// ```
    pub fn with_node(mut self, node: NodeSchema) -> Self {
        self.nodes.insert(node.name.clone(), node);
        self
    }

    /// Adds a name that stands for any of several nodes or unions.
    ///
    /// ```
    /// use langmeld_syntax::Schema;
    ///
    /// let schema = Schema::default().with_union("Expr", ["Literal", "Call"]);
    /// assert_eq!(schema.union("Expr"), Some(&["Literal".to_owned(), "Call".to_owned()][..]));
    /// ```
    pub fn with_union<S: Into<String>>(
        mut self,
        name: impl Into<String>,
        members: impl IntoIterator<Item = S>,
    ) -> Self {
        let members = members.into_iter().map(Into::into).collect();
        self.unions.insert(name.into(), members);
        self
    }

    /// The node called `name`.
    ///
    /// ```
    /// use langmeld_syntax::{NodeSchema, Schema};
    ///
    /// let schema = Schema::default().with_node(NodeSchema::new("Name"));
    /// assert_eq!(schema.node("Name").map(|n| n.name.as_str()), Some("Name"));
    /// assert!(schema.node("Other").is_none());
    /// ```
    pub fn node(&self, name: &str) -> Option<&NodeSchema> {
        self.nodes.get(name)
    }

    /// What the union called `name` stands for.
    ///
    /// ```
    /// use langmeld_syntax::Schema;
    ///
    /// let schema = Schema::default().with_union("Expr", ["Literal"]);
    /// assert_eq!(schema.union("Expr").map(<[String]>::len), Some(1));
    /// assert!(schema.union("Other").is_none());
    /// ```
    pub fn union(&self, name: &str) -> Option<&[String]> {
        self.unions.get(name).map(Vec::as_slice)
    }

    /// Every group of nodes, in name order, with what it stands for.
    ///
    /// ```
    /// use langmeld_syntax::Schema;
    ///
    /// let schema = Schema::default().with_union("Expr", ["Add", "Num"]);
    /// let unions: Vec<_> = schema.unions().map(|(name, _)| name).collect();
    /// assert_eq!(unions, ["Expr"]);
    /// ```
    pub fn unions(&self) -> impl Iterator<Item = (&str, &[String])> {
        self.unions
            .iter()
            .map(|(name, members)| (name.as_str(), members.as_slice()))
    }

    pub(super) fn set_place(&mut self, node: &str, field: &str, place: Option<Place>) {
        let fields = self.nodes.get_mut(node).map(|node| node.fields.iter_mut());
        if let Some(found) = fields.into_iter().flatten().find(|f| f.name == field) {
            found.place = place;
        }
    }

    /// Every node, in name order.
    ///
    /// ```
    /// use langmeld_syntax::{NodeSchema, Schema};
    ///
    /// let schema = Schema::default()
    ///     .with_node(NodeSchema::new("B"))
    ///     .with_node(NodeSchema::new("A"));
    /// let names: Vec<_> = schema.nodes().map(|node| node.name.as_str()).collect();
    /// assert_eq!(names, ["A", "B"]);
    /// ```
    pub fn nodes(&self) -> impl Iterator<Item = &NodeSchema> {
        self.nodes.values()
    }

    /// The nodes that `names` stand for, unions expanded.
    pub(super) fn expand<'a>(
        &'a self,
        names: impl IntoIterator<Item = &'a String>,
    ) -> BTreeSet<&'a str> {
        let mut found = BTreeSet::new();
        let mut pending: Vec<&String> = names.into_iter().collect();
        let mut seen = BTreeSet::new();
        while let Some(name) = pending.pop() {
            if !seen.insert(name) {
                continue;
            }
            match self.unions.get(name) {
                Some(members) => pending.extend(members),
                None => {
                    found.insert(name.as_str());
                }
            }
        }
        found
    }

    /// Compares a tree of well-formed source with the schema: every child must be
    /// allowed, and every field that occurs once must be there. `kind_name` gives the
    /// name of each kind of node, as the schema calls it.
    ///
    /// ```
    /// use langmeld_syntax::{
    ///     Cardinality, FieldSchema, FileId, IdentStyle, Lexer, NodeSchema, Parser, Preset,
    ///     Schema, SourceFile, SyntaxKind, TokenSpec,
    /// };
    ///
    /// const WORD: SyntaxKind = SyntaxKind(1);
    /// const NAME: SyntaxKind = SyntaxKind(2);
    /// const FILE: SyntaxKind = SyntaxKind(3);
    ///
    /// let schema = Schema::default()
    ///     .with_node(NodeSchema::new("Name"))
    ///     .with_node(NodeSchema::new("File").with_field(FieldSchema::new(
    ///         "name",
    ///         ["Name"],
    ///         Cardinality::One,
    ///     )));
    ///
    /// // A hand-written parser whose file has no name.
    /// let lexer = Lexer::builder()
    ///     .token(TokenSpec::preset(WORD, "word", Preset::Ident(IdentStyle::Ascii)))
    ///     .build()
    ///     .expect("the declarations are valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "")?;
    /// let mut parser = Parser::new(&file, lexer.lex(&file), &lexer);
    /// let root = parser.start();
    /// root.complete(&mut parser, FILE);
    ///
    /// let names = |kind: SyntaxKind| match kind {
    ///     NAME => "Name".to_owned(),
    ///     FILE => "File".to_owned(),
    ///     _ => "?".to_owned(),
    /// };
    /// let broken = schema.check(&parser.finish().syntax(), names);
    /// assert_eq!(broken[0].to_string(), "`File` lacks its `name`");
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn check(
        &self,
        root: &SyntaxNode,
        kind_name: impl Fn(SyntaxKind) -> String,
    ) -> Vec<SchemaError> {
        let mut errors = Vec::new();
        for node in root.descendants() {
            if node.kind() == SyntaxKind::ERROR {
                continue;
            }
            let name = kind_name(node.kind());
            let Some(schema) = self.nodes.get(&name) else {
                errors.push(SchemaError {
                    node: name,
                    message: "is not in the schema".to_owned(),
                });
                continue;
            };
            if schema.open {
                continue;
            }
            let allowed = self.expand(&schema.children);
            let children: Vec<_> = node
                .children()
                .filter(|child| child.kind() != SyntaxKind::ERROR)
                .map(|child| kind_name(child.kind()))
                .collect();
            for child in &children {
                if !allowed.contains(child.as_str()) {
                    errors.push(SchemaError {
                        node: name.clone(),
                        message: format!("cannot have `{child}` as a child"),
                    });
                }
            }
            for field in &schema.fields {
                if field.cardinality != Cardinality::One || field.nodes.is_empty() {
                    continue;
                }
                let wanted = self.expand(&field.nodes);
                if !children.iter().any(|child| wanted.contains(child.as_str())) {
                    errors.push(SchemaError {
                        node: name.clone(),
                        message: format!("lacks its `{}`", field.name),
                    });
                }
            }
        }
        errors
    }
}
