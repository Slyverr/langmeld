//! Rust types for the trees of a language, written from its node schema.
//!
//! Every node becomes a struct that views a [`SyntaxNode`](langmeld_syntax::SyntaxNode),
//! with a method for each label; every group of nodes becomes an enum; and every
//! declared token gets a type of its own. A `Visitor` trait has a method for each
//! node and no defaults, so that code that visits trees keeps up with the grammar.
//!
//! Build scripts use [`build::generate`](crate::build::generate); this module is for
//! tools that write the code themselves.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{self, Write};

use langmeld_syntax::{Cardinality, FieldSchema, Language, NodeSchema, SyntaxKind};

/// The code of a language's types.
#[derive(Debug)]
#[non_exhaustive]
pub struct Code {
    /// Rust source to include in a module.
    pub source: String,
    /// Parts of the grammar that get no method, with the reason.
    pub warnings: Vec<String>,
}

/// Why types cannot be written for a grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CodegenError {
    /// Two parts of the grammar would give a type or method of the same name.
    Clash(String),
    /// A name cannot be written as a Rust name.
    Name(String),
}

impl fmt::Display for CodegenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clash(name) => write!(
                f,
                "two parts of the grammar would both be called `{name}` in Rust; rename one"
            ),
            Self::Name(name) => write!(f, "`{name}` cannot be written as a Rust name; rename it"),
        }
    }
}

impl Error for CodegenError {}

/// Words Rust does not allow as plain names.
const KEYWORDS: [&str; 52] = [
    "Self", "abstract", "as", "async", "await", "become", "box", "break", "const", "continue",
    "crate", "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if",
    "impl", "in", "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub",
    "ref", "return", "self", "static", "struct", "super", "trait", "true", "try", "type", "typeof",
    "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];

/// Words that cannot even be written with `r#`.
const NEVER_RAW: [&str; 4] = ["Self", "crate", "self", "super"];

fn is_name(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && text != "_"
}

/// `let_stmt` and `LetStmt` both give `LetStmt`.
fn pascal_case(name: &str) -> String {
    name.split('_')
        .flat_map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_ascii_uppercase())
                .into_iter()
                .chain(chars)
        })
        .collect()
}

fn snake_case(name: &str) -> String {
    let mut snake = String::new();
    for (index, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if index > 0 && !snake.ends_with('_') {
                snake.push('_');
            }
            snake.push(c.to_ascii_lowercase());
        } else {
            snake.push(c);
        }
    }
    snake
}

fn type_name(name: &str) -> Result<String, CodegenError> {
    let written = pascal_case(name);
    if is_name(&written) && !KEYWORDS.contains(&written.as_str()) {
        Ok(written)
    } else {
        Err(CodegenError::Name(name.to_owned()))
    }
}

/// The name of the method for a label. Methods every view has keep their names.
fn method_name(label: &str) -> Result<String, CodegenError> {
    if !is_name(label) {
        return Err(CodegenError::Name(label.to_owned()));
    }
    let taken = ["can_cast", "cast", "range", "syntax"];
    Ok(if NEVER_RAW.contains(&label) || taken.contains(&label) {
        format!("{label}_")
    } else if KEYWORDS.contains(&label) {
        format!("r#{label}")
    } else {
        label.to_owned()
    })
}

const AST: &str = "::langmeld::ast::AstNode";
const NODE: &str = "::langmeld::SyntaxNode";
const TOKEN: &str = "::langmeld::SyntaxToken";
const ELEMENT: &str = "::langmeld::SyntaxElement";
const KIND: &str = "::langmeld::SyntaxKind";
const RANGE: &str = "::langmeld::TextRange";
const OPTION: &str = "::core::option::Option";
const DERIVES: &str = "#[derive(Clone, Debug, PartialEq, Eq, Hash)]";

/// A method call chain, on one line if it is short and one call to a line if not, as
/// `rustfmt` writes it.
fn chain(root: &str, links: &[String]) -> String {
    let one_line = format!("{root}{}", links.concat());
    if one_line.len() <= 60 {
        return one_line;
    }
    let lines: Vec<_> = links
        .iter()
        .map(|link| format!("\n            {link}"))
        .collect();
    format!("{root}{}", lines.concat())
}

struct Writer<'l> {
    language: &'l Language,
    out: String,
    /// Types written or to be written, to refuse two of a name.
    types: BTreeSet<String>,
    /// Nodes and groups of nodes by grammar name, with their type.
    views: BTreeMap<String, String>,
    /// Declared tokens by name, with their type.
    tokens: BTreeMap<String, String>,
    /// Groups made for fields that can be one of several nodes.
    groups: Vec<(String, Vec<String>)>,
    warnings: Vec<String>,
}

impl Writer<'_> {
    fn claim(&mut self, name: &str) -> Result<(), CodegenError> {
        if name == "Visitor" || !self.types.insert(name.to_owned()) {
            return Err(CodegenError::Clash(name.to_owned()));
        }
        Ok(())
    }

    /// The kind of a node, if the language makes it.
    fn node_kind(&self, name: &str) -> Option<SyntaxKind> {
        self.language
            .kind(name)
            .filter(|&kind| self.language.kind_name(kind) == name)
    }

    fn token(&mut self, name: &str, view: &str, kind: SyntaxKind) {
        let kind = kind.0;
        let _ = write!(
            self.out,
            "
/// The `{name}` token.
{DERIVES}
pub struct {view}({TOKEN});

impl {view} {{
    /// The view of `token`, if it is the `{name}` token.
    pub fn cast(token: {TOKEN}) -> {OPTION}<Self> {{
        (token.kind() == {KIND}({kind})).then(|| Self(token))
    }}

    /// The token viewed.
    pub fn syntax(&self) -> &{TOKEN} {{
        &self.0
    }}

    /// The text of the token.
    pub fn text(&self) -> &str {{
        self.0.text()
    }}

    /// Where the token is in the source.
    pub fn range(&self) -> {RANGE} {{
        self.0.text_range()
    }}
}}
"
        );
    }

    fn group(&mut self, view: &str, about: &str, members: &[String]) {
        let _ = write!(self.out, "\n/// {about}\n{DERIVES}\npub enum {view} {{\n");
        for member in members {
            let _ = writeln!(
                self.out,
                "    /// The `{member}` node.\n    {member}({member}),"
            );
        }
        let can_cast: Vec<_> = members
            .iter()
            .map(|member| format!("{member}::can_cast(kind)"))
            .collect();
        let _ = write!(
            self.out,
            "}}

impl {AST} for {view} {{
    fn can_cast(kind: {KIND}) -> bool {{
        {}
    }}

    fn cast(node: {NODE}) -> {OPTION}<Self> {{
",
            {
                let one_line = can_cast.join(" || ");
                if one_line.len() + 8 <= 100 {
                    one_line
                } else {
                    can_cast.join("\n            || ")
                }
            }
        );
        for member in members {
            let _ = write!(
                self.out,
                "        if {member}::can_cast(node.kind()) {{
            return {member}::cast(node).map(Self::{member});
        }}
"
            );
        }
        let _ = write!(
            self.out,
            "        ::core::option::Option::None
    }}

    fn syntax(&self) -> &{NODE} {{
        match self {{
"
        );
        for member in members {
            let _ = writeln!(self.out, "            Self::{member}(it) => it.syntax(),");
        }
        self.out.push_str("        }\n    }\n}\n");
    }

    /// The type a field gives, and the steps that turn the children into it.
    fn field_type(
        &mut self,
        node: &str,
        field: &FieldSchema,
    ) -> Result<Option<(String, Vec<String>)>, CodegenError> {
        let views: Vec<_> = field
            .nodes
            .iter()
            .filter_map(|name| self.views.get(name).cloned())
            .collect();
        // Custom code may make nodes that the grammar does not name.
        let any_node = views.len() < field.nodes.len();
        let kinds: Vec<_> = field
            .tokens
            .iter()
            .filter_map(|name| self.language.token_kind(name))
            .map(|kind| format!("{}", kind.0))
            .collect();
        let token_test = format!("matches!(it.kind().0, {})", kinds.join(" | "));
        let into_node = format!(".filter_map({ELEMENT}::into_node)");
        let into_token = format!(".filter_map({ELEMENT}::into_token)");
        if kinds.is_empty() {
            return Ok(match (views.as_slice(), any_node) {
                ([], false) => None,
                ([view], false) => Some((
                    view.clone(),
                    vec![into_node, format!(".filter_map({view}::cast)")],
                )),
                (_, false) => {
                    let name = format!("{node}{}", type_name(&field.name)?);
                    self.claim(&name)?;
                    self.groups.push((name.clone(), views));
                    Some((
                        name.clone(),
                        vec![into_node, format!(".filter_map({name}::cast)")],
                    ))
                }
                _ => Some((NODE.to_owned(), vec![into_node])),
            });
        }
        if field.nodes.is_empty() {
            let single = match field.tokens.as_slice() {
                [name] => self.tokens.get(name).cloned(),
                _ => None,
            };
            return Ok(Some(match single {
                Some(view) => (
                    view.clone(),
                    vec![into_token, format!(".filter_map({view}::cast)")],
                ),
                None => (
                    TOKEN.to_owned(),
                    vec![into_token, format!(".filter(|it| {token_test})")],
                ),
            }));
        }
        let mut tests: Vec<_> = views
            .iter()
            .map(|view| format!("{view}::can_cast(it.kind())"))
            .collect();
        if any_node {
            tests = vec!["it.as_node().is_some()".to_owned()];
        }
        tests.push(format!("(it.as_token().is_some() && {token_test})"));
        Ok(Some((
            ELEMENT.to_owned(),
            vec![format!(".filter(|it| {})", tests.join(" || "))],
        )))
    }

    fn node(
        &mut self,
        schema: &NodeSchema,
        view: &str,
        kind: SyntaxKind,
    ) -> Result<(), CodegenError> {
        let name = &schema.name;
        let kind = kind.0;
        let _ = write!(
            self.out,
            "
/// The `{name}` node.
{DERIVES}
pub struct {view}({NODE});

impl {AST} for {view} {{
    fn can_cast(kind: {KIND}) -> bool {{
        kind.0 == {kind}
    }}

    fn cast(node: {NODE}) -> {OPTION}<Self> {{
        Self::can_cast(node.kind()).then(|| Self(node))
    }}

    fn syntax(&self) -> &{NODE} {{
        &self.0
    }}
}}
"
        );
        let mut methods = BTreeSet::new();
        let mut body = String::new();
        for field in &schema.fields {
            let label = &field.name;
            let method = method_name(label)?;
            if !methods.insert(method.clone()) {
                return Err(CodegenError::Clash(format!("{view}::{method}")));
            }
            let Some(place) = &field.place else {
                self.warnings.push(format!(
                    "`{name}` gets no method for `{label}`: a tree cannot tell it apart \
                     from other parts; separate them with a literal or put one in a rule \
                     of its own"
                ));
                continue;
            };
            let Some((result, cast)) = self.field_type(view, field)? else {
                continue;
            };
            let bound = |text: &Option<String>| {
                text.as_ref()
                    .and_then(|text| self.language.token_kind(text))
                    .map(|kind| kind.0)
            };
            let (after, before) = (bound(&place.after), bound(&place.before));
            let many = field.cardinality == Cardinality::Many;
            let returns = if many {
                format!("impl ::core::iter::Iterator<Item = {result}> + use<>")
            } else {
                format!("{OPTION}<{result}>")
            };
            let root = match (after, before) {
                (None, None) => "self.0".to_owned(),
                (Some(after), None) => format!("::langmeld::ast::after(&self.0, {after})"),
                (None, Some(before)) => format!("::langmeld::ast::before(&self.0, {before})"),
                (Some(after), Some(before)) => {
                    format!("::langmeld::ast::within(&self.0, {after}, {before})")
                }
            };
            let mut links = cast;
            if after.is_none() && before.is_none() {
                links.insert(0, ".children_with_tokens()".to_owned());
            }
            let end = match (many, place.skip) {
                (true, 0) => None,
                (true, skip) => Some(format!(".skip({skip})")),
                (false, 0) => match links.pop() {
                    // The first one that passes a test is found.
                    Some(last) if last.starts_with(".filter(") => {
                        Some(last.replacen(".filter(", ".find(", 1))
                    }
                    Some(last) => {
                        links.push(last);
                        Some(".next()".to_owned())
                    }
                    None => Some(".next()".to_owned()),
                },
                (false, skip) => Some(format!(".nth({skip})")),
            };
            links.extend(end);
            let expression = chain(&root, &links);
            let _ = write!(
                body,
                "
    /// The `{label}` part.
    pub fn {method}(&self) -> {returns} {{
        {expression}
    }}
"
            );
        }
        if !body.is_empty() {
            let _ = write!(self.out, "\nimpl {view} {{{body}}}\n");
        }
        Ok(())
    }

    fn visitor(&mut self, nodes: &[(String, u16)]) -> Result<(), CodegenError> {
        self.out.push_str(
            "
/// Handles each kind of node. No method has a default, so that code visiting trees
/// stops compiling when the grammar gains a node.
pub trait Visitor {
    /// What handling a node gives.
    type Output;
",
        );
        let mut methods = BTreeSet::new();
        let mut arms = String::new();
        for (view, kind) in nodes {
            let method = format!("visit_{}", snake_case(view));
            if !methods.insert(method.clone()) {
                return Err(CodegenError::Clash(method));
            }
            let _ = write!(
                self.out,
                "
    /// Handles a `{view}`.
    fn {method}(&mut self, node: &{view}) -> Self::Output;
"
            );
            let _ = writeln!(
                arms,
                "        {kind} => {view}::cast(node).map(|it| visitor.{method}(&it)),"
            );
        }
        let _ = write!(
            self.out,
            "}}

/// Gives `node` to the method of `visitor` for its kind. Nodes of unexpected text
/// have none.
pub fn visit<V: Visitor + ?Sized>(
    visitor: &mut V,
    node: {NODE},
) -> {OPTION}<V::Output> {{
    match node.kind().0 {{
{arms}        _ => {OPTION}::None,
    }}
}}
"
        );
        Ok(())
    }

    fn loader(&mut self) {
        let custom: Vec<_> = self
            .language
            .custom_rules()
            .chain(self.language.custom_tokens())
            .map(|name| format!("`{name}`"))
            .collect();
        let bytes = "include_bytes!(concat!(env!(\"OUT_DIR\"), \"/language.bin\"))";
        let about = if custom.is_empty() {
            "The language, as the build script compiled it.".to_owned()
        } else {
            format!(
                "The language, as the build script compiled it. Give the code of {} with \
                 `extern_rule` and `extern_token` before loading it.",
                custom.join(", ")
            )
        };
        let _ = write!(
            self.out,
            "
/// {about}
pub fn source() -> ::langmeld::Source {{
    ::langmeld::Source::embedded({bytes})
}}
"
        );
        if custom.is_empty() {
            self.out.push_str(
                "
/// The language and what it tells editors, read once.
pub fn loaded() -> &'static ::langmeld::Loaded {
    static LOADED: ::std::sync::OnceLock<::langmeld::Loaded> = ::std::sync::OnceLock::new();
    LOADED.get_or_init(|| {
        ::langmeld::load_language(source()).expect(\"the build script compiled it with this version\")
    })
}

/// The language, read once.
pub fn language() -> &'static ::langmeld::Language {
    &loaded().language
}
",
            );
        }
    }
}

/// Writes the types of a language. The code uses the `langmeld` crate, and its
/// `language` function reads what [`build::generate`](crate::build::generate) writes
/// beside it.
pub fn generate(language: &Language) -> Result<Code, CodegenError> {
    let schema = language.schema();
    let mut writer = Writer {
        language,
        out: String::from(
            "// Written by Langmeld from the grammar. Changes are lost.\n\nuse ::langmeld::ast::AstNode as _;\n",
        ),
        types: BTreeSet::new(),
        views: BTreeMap::new(),
        tokens: BTreeMap::new(),
        groups: Vec::new(),
        warnings: Vec::new(),
    };

    let mut nodes = Vec::new();
    for node in schema.nodes() {
        // Custom code may describe nodes the grammar never makes.
        let Some(kind) = writer.node_kind(&node.name) else {
            continue;
        };
        let view = type_name(&node.name)?;
        writer.claim(&view)?;
        writer.views.insert(node.name.clone(), view.clone());
        nodes.push((node, view, kind));
    }
    // Groups of nodes, including groups of groups.
    let mut unions: Vec<(&str, &[String])> = schema.unions().collect();
    loop {
        let before = unions.len();
        let mut waiting = Vec::new();
        for (name, members) in unions {
            let known = members.iter().all(|m| writer.views.contains_key(m));
            if known && !members.is_empty() {
                let view = type_name(name)?;
                writer.claim(&view)?;
                writer.views.insert(name.to_owned(), view.clone());
                let members = members
                    .iter()
                    .filter_map(|m| writer.views.get(m).cloned())
                    .collect();
                writer.groups.push((view, members));
            } else {
                waiting.push((name, members));
            }
        }
        unions = waiting;
        if unions.len() == before {
            break;
        }
    }
    let named_groups = writer.groups.len();

    for name in language.declared_tokens() {
        // A string with holes is a node.
        let Some(kind) = language.token_kind(name) else {
            continue;
        };
        if schema.node(name).is_some() {
            continue;
        }
        let view = format!("{}Token", type_name(name)?);
        writer.claim(&view)?;
        writer.tokens.insert(name.to_owned(), view.clone());
        writer.token(name, &view, kind);
    }
    for (node, view, kind) in &nodes {
        writer.node(node, view, *kind)?;
    }
    for (index, (view, members)) in std::mem::take(&mut writer.groups).into_iter().enumerate() {
        let about = if index < named_groups {
            format!("One of the nodes `{view}` stands for.")
        } else {
            "One of the nodes a part can be.".to_owned()
        };
        writer.group(&view, &about, &members);
    }
    let visited: Vec<_> = nodes
        .iter()
        .map(|(_, view, kind)| (view.clone(), kind.0))
        .collect();
    writer.visitor(&visited)?;
    writer.loader();
    Ok(Code {
        source: writer.out,
        warnings: writer.warnings,
    })
}
