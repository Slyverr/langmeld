//! Languages declared as grammars.

mod analyze;
mod builder;
mod compile;
mod engine;
mod ir;
pub mod pattern;
mod problem;
mod recovery;
mod schema;
mod schema_of;

use std::collections::HashMap;
use std::fmt;

use crate::{Diagnostic, Lexer, Parse, ParseLimits, Parser, Severity, SourceFile, SyntaxKind};

pub use builder::{Assoc, ExprSpec, ExternRule, LanguageBuilder, Token};
pub use pattern::Pattern;
pub use schema::{Cardinality, FieldSchema, NodeSchema, Schema, SchemaError};

use ir::{Callee, Expr, Extern, KindSet, Node, Rule};

/// A language declared as a grammar, checked and ready to parse.
///
/// Declare it with [`Language::builder`]; [`parse`] runs it on a file.
///
/// ```
/// use langmeld_syntax::pattern::{field, lit, many, r};
/// use langmeld_syntax::{
///     FileId, IdentStyle, Language, Preset, SourceFile, Token, parse, seq,
/// };
///
/// let language = Language::builder("Settings")
///     .token(Token::skip("space", r"\s+"))
///     .token(Token::preset("word", Preset::Ident(IdentStyle::Xid)))
///     .rule("File", many(r("Setting")))
///     .rule(
///         "Setting",
///         seq![field("key", r("word")), lit("="), field("value", r("word")), lit(";")],
///     )
///     .build()
///     .expect("the grammar is valid");
///
/// let file = SourceFile::new(FileId::new(0), "example", "colour = blue;")?;
/// let parsed = parse(&language, &file);
/// assert!(parsed.diagnostics.is_empty());
/// let setting = parsed.syntax().first_child().expect("one setting");
/// assert_eq!(language.kind_name(setting.kind()), "Setting");
/// # Ok::<(), langmeld_syntax::TextTooLarge>(())
/// ```
pub struct Language {
    name: String,
    lexer: Lexer,
    nodes: Vec<Node>,
    rules: Vec<Rule>,
    exprs: Vec<Expr>,
    externs: Vec<Extern>,
    root: Callee,
    kind_names: Vec<String>,
    node_kinds: HashMap<String, SyntaxKind>,
    /// Declared tokens by name, and literals by text.
    token_kinds: HashMap<String, SyntaxKind>,
    callees: HashMap<String, Callee>,
    /// The kind of a line ending that terminates, if lines do.
    newline: Option<SyntaxKind>,
    /// Literals that terminate as a line ending does.
    terminators: Vec<ir::Symbol>,
    /// The kinds that close a block, before which a terminator may be left out.
    closers: KindSet,
    /// Contextual words, which are identifiers until a rule asks for them.
    words: HashMap<String, SyntaxKind>,
    parse_limits: ParseLimits,
    schema: Schema,
    warnings: Vec<Diagnostic>,
}

impl Language {
    /// Starts declaring a language called `name`.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{lit, r};
    /// use langmeld_syntax::{IdentStyle, Language, Preset, Token, seq};
    ///
    /// let language = Language::builder("Greeting")
    ///     .token(Token::skip("space", r"\s+"))
    ///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
    ///     .rule("File", seq![lit("hello"), r("word")])
    ///     .build()
    ///     .expect("the grammar is valid");
    /// assert!(language.warnings().is_empty());
    /// ```
    pub fn builder(name: impl Into<String>) -> LanguageBuilder {
        LanguageBuilder::new(name.into())
    }

    /// The kind of the node, token or literal called `name`. Nodes are named for the
    /// rules, atoms and operators that make them; a literal is named by its text.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{lit, many};
    /// use langmeld_syntax::Language;
    ///
    /// let language = Language::builder("Bangs")
    ///     .rule("File", many(lit("!")))
    ///     .build()
    ///     .expect("the grammar is valid");
    /// assert!(language.kind("File").is_some());
    /// assert!(language.kind("!").is_some());
    /// assert_eq!(language.kind("Missing"), None);
    /// ```
    pub fn kind(&self, name: &str) -> Option<SyntaxKind> {
        self.node_kinds
            .get(name)
            .or_else(|| self.token_kinds.get(name))
            .copied()
    }

    /// The name of a kind: a node's or token's name, or a literal's text.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{lit, many};
    /// use langmeld_syntax::{Language, SyntaxKind};
    ///
    /// let language = Language::builder("Bangs")
    ///     .rule("File", many(lit("!")))
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = language.kind("File").expect("a rule");
    /// assert_eq!(language.kind_name(file), "File");
    /// assert_eq!(language.kind_name(SyntaxKind::ERROR), "Error");
    /// ```
    pub fn kind_name(&self, kind: SyntaxKind) -> &str {
        match kind {
            SyntaxKind::ERROR => "Error",
            SyntaxKind::EOF => "end of file",
            SyntaxKind::BOM => "byte order mark",
            kind => self
                .kind_names
                .get(usize::from(kind.0))
                .map_or("unknown", String::as_str),
        }
    }

    /// Whether tokens of `kind` are trivia, such as whitespace and comments, which rules
    /// never see but the tree keeps.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{lit, many, r};
    /// use langmeld_syntax::{Language, Token};
    ///
    /// let language = Language::builder("Bangs")
    ///     .token(Token::skip("space", r"\s+"))
    ///     .rule("File", many(lit("!")))
    ///     .build()
    ///     .expect("the grammar is valid");
    /// assert!(language.is_trivia(language.kind("space").expect("declared")));
    /// assert!(!language.is_trivia(language.kind("!").expect("used")));
    /// ```
    pub fn is_trivia(&self, kind: SyntaxKind) -> bool {
        self.lexer.is_trivia(kind)
    }

    /// The lexer that splits files into the tokens the rules read.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{lit, many};
    /// use langmeld_syntax::{FileId, Language, SourceFile};
    ///
    /// let language = Language::builder("Bangs")
    ///     .rule("File", many(lit("!")))
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = SourceFile::new(FileId::new(0), "example", "!!")?;
    /// assert!(language.lexer().lex(&file).diagnostics.is_empty());
    /// # Ok::<(), langmeld_syntax::TextTooLarge>(())
    /// ```
    pub fn lexer(&self) -> &Lexer {
        &self.lexer
    }

    /// The nodes of the language, as worked out from the labels in its rules.
    ///
    /// ```
    /// use langmeld_syntax::pattern::{field, lit, many, r};
    /// use langmeld_syntax::{Cardinality, IdentStyle, Language, Preset, Token, seq};
    ///
    /// let language = Language::builder("Lists")
    ///     .token(Token::skip("space", r"\s+"))
    ///     .token(Token::preset("word", Preset::Ident(IdentStyle::Ascii)))
    ///     .rule("File", seq![lit("list"), field("items", many(r("word")))])
    ///     .build()
    ///     .expect("the grammar is valid");
    /// let file = language.schema().node("File").expect("a rule is a node");
    /// assert_eq!(file.field("items").expect("labelled").cardinality, Cardinality::Many);
    /// ```
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Problems that do not stop the grammar from being used, such as a rule that
    /// nothing uses.
    ///
    /// ```
    /// use langmeld_syntax::pattern::lit;
    /// use langmeld_syntax::Language;
    ///
    /// let language = Language::builder("Spare")
    ///     .rule("File", lit("!"))
    ///     .rule("Unused", lit("?"))
    ///     .build()
    ///     .expect("only a warning");
    /// assert_eq!(language.warnings()[0].code, "LM-G006");
    /// ```
    pub fn warnings(&self) -> &[Diagnostic] {
        &self.warnings
    }
}

impl fmt::Debug for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rules: Vec<_> = self.rules.iter().map(|rule| rule.name.as_str()).collect();
        let externs: Vec<_> = self.externs.iter().map(|e| e.name.as_str()).collect();
        f.debug_struct("Language")
            .field("name", &self.name)
            .field("rules", &rules)
            .field("externs", &externs)
            .finish_non_exhaustive()
    }
}

/// Parses a file of a language. Parsing never fails: the tree holds every byte, and
/// what is wrong is in the diagnostics.
///
/// In a debug build, a tree that the language's own code made and the schema does not
/// allow is reported as `LM-S001`.
///
/// ```
/// use langmeld_syntax::pattern::{lit, many};
/// use langmeld_syntax::{FileId, Language, SourceFile, Token, parse};
///
/// let language = Language::builder("Bangs")
///     .token(Token::skip("space", r"\s+"))
///     .rule("File", many(lit("!")))
///     .build()
///     .expect("the grammar is valid");
/// let file = SourceFile::new(FileId::new(0), "example", "! ? !")?;
/// let parsed = parse(&language, &file);
/// // The tree holds every byte; the `?` is reported once.
/// assert_eq!(parsed.syntax().text(), "! ? !");
/// assert_eq!(parsed.diagnostics.len(), 1);
/// # Ok::<(), langmeld_syntax::TextTooLarge>(())
/// ```
pub fn parse(language: &Language, file: &SourceFile) -> Parse {
    let lexed = language.lexer.lex(file);
    let engine = engine::Engine::new(language);
    let mut parser = Parser::new(file, lexed, &language.lexer)
        .limits(language.parse_limits)
        .rules(&engine);
    engine.file(&mut parser);
    let mut parsed = parser.finish();
    if cfg!(debug_assertions) && parsed.diagnostics.is_empty() {
        check_schema(language, file, &mut parsed);
    }
    parsed
}

/// Code of the language's own can make nodes the grammar does not describe. Debug
/// builds say so, as a diagnostic because parsing must not panic.
fn check_schema(language: &Language, file: &SourceFile, parsed: &mut Parse) {
    let root = parsed.syntax();
    let names = |kind| language.kind_name(kind).to_owned();
    let Some(broken) = language.schema.check(&root, names).into_iter().next() else {
        return;
    };
    let range = root
        .descendants()
        .find(|node| language.kind_name(node.kind()) == broken.node)
        .map_or_else(|| root.text_range(), |node| node.text_range());
    parsed.diagnostics.push(
        Diagnostic::new(
            Severity::Error,
            "LM-S001",
            file.id(),
            range,
            format!("the grammar's code made a wrong tree: {broken}"),
        )
        .note(
            "To fix this, declare the node with `ExternRule::produces`, or change the code so \
             that it only makes nodes the grammar describes.",
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::ir::Entry;
    use super::*;
    use crate::pattern::{choice, many, r};
    use crate::seq;
    use crate::{IdentStyle, Preset};

    /// A language of `count` ways to start an item, each with its own word.
    fn ways(count: usize) -> Language {
        let mut builder = Language::builder("Ways")
            .token(Token::skip("space", r"\s+"))
            .token(Token::preset("ident", Preset::Ident(IdentStyle::Ascii)))
            .rule("File", many(r("Item")))
            .rule("Item", choice((0..count).map(|n| r(format!("W{n}")))));
        for n in 0..count {
            builder = builder.rule(
                format!("W{n}"),
                seq![crate::pattern::lit(format!("w{n}")), r("ident")],
            );
        }
        builder.build().expect("the grammar is valid")
    }

    #[test]
    fn a_choice_is_one_lookup_in_a_table_whatever_the_number_of_ways() {
        for count in [5, 50, 300] {
            let language = ways(count);
            let table = language
                .nodes
                .iter()
                .find_map(|node| match node {
                    Node::Choice {
                        alternatives,
                        table,
                        ..
                    } if alternatives.len() == count => Some(table),
                    _ => None,
                })
                .expect("the choice of ways");
            for n in 0..count {
                let word = language.kind(&format!("w{n}")).expect("a literal");
                // The first token alone settles it: no deeper table, no search.
                assert!(
                    matches!(table.kinds.get(usize::from(word.0)), Some(Entry::Take(way)) if usize::from(*way) == n),
                    "{count} ways, way {n}"
                );
            }
        }
    }

    #[test]
    fn recovery_sets_are_sets_of_kinds_made_before_parsing() {
        let language = ways(5);
        let loops: Vec<_> = language
            .nodes
            .iter()
            .filter_map(|node| match node {
                Node::Many { stop, .. } => Some(stop),
                _ => None,
            })
            .collect();
        assert_eq!(loops.len(), 1);
        // The repetition can resume where an item starts and where the file ends.
        let first = language.kind("w0").expect("a literal");
        assert!(loops[0].contains(first));
        assert!(loops[0].contains(SyntaxKind::EOF));
        assert!(!loops[0].contains(SyntaxKind(999)));
    }
}
