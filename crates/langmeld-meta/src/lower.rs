//! Turns the declarations of `.langmeld` files into calls of the grammar builder, and
//! reports what is wrong with the files themselves, each at its place in them.

use std::collections::{HashMap, HashSet};

use langmeld_syntax::pattern::{
    attempt, choice, field, lit, look, many, many1, not, opt, r, sep, sep_strict, seq, terminated,
};
use langmeld_syntax::{
    Assoc, Diagnostic, ExprSpec, ExternRule, Externals, FileId, Fix, FloatPreset, IdentStyle,
    IntPreset, LanguageBuilder, Parser, Pattern, Preset, SourceFile, StringPreset, SyntaxKind,
    SyntaxToken, TextEdit, TextRange, Token,
};

use crate::declared::{BindRules, Binding, Editor, EditorRule, Path, Place};
use crate::problem::problem;
use crate::tree::Node;

/// The version of the format this release reads.
pub(crate) const VERSION: &str = "1";

/// One file of a language, with its tree.
pub(crate) struct Unit {
    pub(crate) file: SourceFile,
    pub(crate) root: Node,
}

/// How to treat what Rust has to provide.
pub(crate) struct Options<'a> {
    pub(crate) externals: &'a Externals,
    /// Without a Rust host, custom rules are read as one error each.
    pub(crate) generic: bool,
}

/// Everything the files declare.
pub(crate) struct Lowered {
    pub(crate) builder: LanguageBuilder,
    pub(crate) extensions: Vec<String>,
    pub(crate) bindings: BindRules,
    pub(crate) editor: Editor,
    pub(crate) problems: Vec<Diagnostic>,
}

const PRESETS: &str = "use one of the presets `int`, `float`, `string`, `char` and `ident`";

fn is_pascal(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && name.chars().all(|c| c.is_ascii_alphanumeric())
}

fn is_snake(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

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
    let chars: Vec<char> = name.chars().collect();
    let mut snake = String::new();
    for (index, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() {
            let after_lower = index > 0 && !chars[index - 1].is_ascii_uppercase();
            let ends_acronym = index > 0
                && chars[index - 1].is_ascii_uppercase()
                && chars.get(index + 1).is_some_and(char::is_ascii_lowercase);
            if (after_lower || ends_acronym) && !snake.ends_with('_') {
                snake.push('_');
            }
            snake.push(c.to_ascii_lowercase());
        } else {
            snake.push(c);
        }
    }
    snake
}

fn text(token: Option<SyntaxToken>) -> String {
    token.map_or_else(String::new, |token| token.text().to_owned())
}

struct Lowering<'a> {
    options: &'a Options<'a>,
    builder: Option<LanguageBuilder>,
    file: FileId,
    /// Names of every rule and expression, in all files.
    rules: HashSet<String>,
    /// Where each name is written, in all files, so that a rename can reach its uses.
    names: HashMap<String, Vec<(FileId, TextRange)>>,
    extensions: Vec<String>,
    bindings: BindRules,
    editor: Editor,
    problems: Vec<Diagnostic>,
}

fn place(file: FileId, range: TextRange) -> Place {
    Place {
        file: file.index(),
        start: u32::from(range.start()),
        end: u32::from(range.end()),
    }
}

impl Lowering<'_> {
    fn with(&mut self, change: impl FnOnce(LanguageBuilder) -> LanguageBuilder) {
        let builder = self
            .builder
            .take()
            .expect("the builder is put back after every step");
        self.builder = Some(change(builder));
    }

    fn report(&mut self, code: &'static str, range: TextRange, headline: String, fix: &str) {
        let found = problem(code, self.file, range, headline, fix);
        self.problems.push(found);
    }

    /// Reports a name written against the convention, with the rename as a fix.
    fn convention(
        &mut self,
        code: &'static str,
        token: &SyntaxToken,
        everywhere: bool,
        better: String,
        what: &str,
        reason: &str,
    ) {
        let name = token.text();
        let headline = format!("the {what} name `{name}` should be written like `{better}`");
        let fix = format!("rename it to `{better}`; {reason}");
        let mut found = problem(code, self.file, token.text_range(), headline, fix);
        let places = match self.names.get(name) {
            Some(places) if everywhere => places.clone(),
            _ => vec![(self.file, token.text_range())],
        };
        let edits = places
            .into_iter()
            .map(|(file, range)| TextEdit::new(file, range, better.clone()))
            .collect();
        found
            .fixes
            .push(Fix::new(format!("rename to `{better}`"), edits));
        self.problems.push(found);
    }

    /// A rule, expression or operator name: it becomes a type.
    fn type_name(&mut self, what: &str, token: &SyntaxToken) {
        if !is_pascal(token.text()) {
            let better = pascal_case(token.text());
            self.convention(
                "LM-F003",
                token,
                true,
                better,
                what,
                "rule names become types, so they start with a capital and have no underscores",
            );
        }
    }

    /// A token or label name: it becomes a method or a field.
    fn lower_name(&mut self, what: &str, token: &SyntaxToken, everywhere: bool) {
        if !is_snake(token.text()) {
            let better = snake_case(token.text());
            self.convention(
                "LM-F004",
                token,
                everywhere,
                better,
                what,
                "tokens and labels use lower case letters and underscores",
            );
        }
    }

    /// The text of a quoted string, escapes replaced.
    fn string(&mut self, token: &SyntaxToken) -> String {
        let quoted = token.text();
        let inner = quoted.strip_prefix('"').unwrap_or(quoted);
        let inner = inner.strip_suffix('"').unwrap_or(inner);
        let mut chars = inner.chars();
        let mut value = String::new();
        while let Some(c) = chars.next() {
            if c != '\\' {
                value.push(c);
                continue;
            }
            match chars.next() {
                Some('n') => value.push('\n'),
                Some('r') => value.push('\r'),
                Some('t') => value.push('\t'),
                Some(c @ ('"' | '\\')) => value.push(c),
                other => {
                    let written = other.map_or_else(String::new, String::from);
                    self.report(
                        "LM-F010",
                        token.text_range(),
                        format!("`\\{written}` is not an escape the format knows"),
                        "use `\\\"`, `\\\\`, `\\n`, `\\r` or `\\t`, or write the character itself",
                    );
                }
            }
        }
        value
    }

    fn strings(&mut self, node: &Node) -> Vec<String> {
        let tokens: Vec<_> = node.all("string").collect();
        tokens.iter().map(|token| self.string(token)).collect()
    }

    fn pattern_of(&mut self, node: &Node, what: &str) -> Pattern {
        match node.child(what) {
            Some(inner) => self.alternation(&inner),
            None => seq([]),
        }
    }

    fn alternation(&mut self, node: &Node) -> Pattern {
        let mut ways: Vec<_> = node
            .children()
            .filter(|child| child.is("Sequence"))
            .map(|child| self.sequence(&child))
            .collect();
        if ways.len() == 1 {
            ways.remove(0)
        } else {
            choice(ways)
        }
    }

    fn sequence(&mut self, node: &Node) -> Pattern {
        let mut parts: Vec<_> = node.children().map(|piece| self.piece(&piece)).collect();
        if parts.len() == 1 {
            parts.remove(0)
        } else {
            seq(parts)
        }
    }

    fn piece(&mut self, node: &Node) -> Pattern {
        let repeated = match node.child("Repeated") {
            Some(repeated) => self.repeated(&repeated),
            None => seq([]),
        };
        if node.is("Look") {
            return if node.has("&") {
                look(repeated)
            } else {
                not(repeated)
            };
        }
        match node.ident() {
            Some(label) => {
                self.lower_name("label", &label, false);
                field(label.text(), repeated)
            }
            None => repeated,
        }
    }

    fn repeated(&mut self, node: &Node) -> Pattern {
        let atom = node
            .children()
            .next()
            .map_or_else(|| seq([]), |atom| self.atom(&atom));
        if node.has("*") {
            many(atom)
        } else if node.has("+") {
            many1(atom)
        } else if node.has("?") {
            opt(atom)
        } else {
            atom
        }
    }

    fn atom(&mut self, node: &Node) -> Pattern {
        match node.name() {
            "Literal" => {
                let written = node.all("string").next();
                let (value, range) = match &written {
                    Some(token) => (self.string(token), Some(token.text_range())),
                    None => (String::new(), None),
                };
                let pattern = lit(value);
                match range {
                    Some(range) => pattern.located(self.file, range),
                    None => pattern,
                }
            }
            "Reference" => {
                let name = node.ident();
                let range = name.as_ref().map(SyntaxToken::text_range);
                let pattern = r(text(name));
                match range {
                    Some(range) => pattern.located(self.file, range),
                    None => pattern,
                }
            }
            "Group" => self.pattern_of(node, "Alternation"),
            "SepList" => {
                let item = self.pattern_of(node, "Alternation");
                let separator = node
                    .all("string")
                    .next()
                    .map_or_else(String::new, |token| self.string(&token));
                if node.has("sep_strict") {
                    sep_strict(item, separator)
                } else {
                    sep(item, separator)
                }
            }
            "Terminated" => terminated(self.pattern_of(node, "Alternation")),
            "Attempt" => attempt(self.pattern_of(node, "Alternation")),
            _ => seq([]),
        }
    }

    fn precedence(&mut self, node: &Node) -> u16 {
        let Some(number) = node.all("number").next() else {
            return 1;
        };
        match number.text().parse::<u16>() {
            Ok(precedence) if precedence > 0 => precedence,
            _ => {
                self.report(
                    "LM-F011",
                    number.text_range(),
                    format!("`{}` is not a precedence", number.text()),
                    "write a whole number from 1 to 65535; a larger one binds tighter",
                );
                1
            }
        }
    }

    fn expression(&mut self, node: &Node) {
        let Some(name) = node.ident() else { return };
        self.type_name("expression", &name);
        let mut spec = ExprSpec::new(name.text());
        for entry in node.children() {
            let Some(entry_name) = entry.ident() else {
                continue;
            };
            self.type_name("operand and operator", &entry_name);
            let entry_name = entry_name.text();
            spec = match entry.name() {
                "AtomDecl" => {
                    let body = self.pattern_of(&entry, "Alternation");
                    spec.atom(entry_name, body)
                }
                kind @ ("PrefixDecl" | "InfixDecl") => {
                    let operators = entry.child("Operators");
                    let symbols = operators.map_or_else(Vec::new, |node| self.strings(&node));
                    let precedence = self.precedence(&entry);
                    if kind == "PrefixDecl" {
                        spec.prefix(entry_name, symbols, precedence)
                    } else {
                        let assoc = match entry.child("Assoc") {
                            Some(word) if word.has("right") => Assoc::Right,
                            Some(word) if word.has("none") => Assoc::None,
                            _ => Assoc::Left,
                        };
                        spec.infix(entry_name, symbols, precedence, assoc)
                    }
                }
                _ => {
                    let body = match entry.child("Sequence") {
                        Some(sequence) => self.sequence(&sequence),
                        None => seq([]),
                    };
                    let precedence = self.precedence(&entry);
                    spec.postfix(entry_name, body, precedence)
                }
            };
        }
        let (file, range) = (self.file, name.text_range());
        self.with(|builder| builder.expr(spec).locate(name.text(), file, range));
    }

    fn preset(&mut self, name: &str, node: &Node) -> Option<Token> {
        let kind = node.ident()?;
        let options: Vec<Node> = node.children().collect();
        let allowed: &[&str] = match kind.text() {
            "int" => &["dec", "hex", "bin", "oct", "underscores"],
            "float" => &["exponent", "underscores"],
            "string" => &["double", "single", "raw"],
            "char" => &[],
            "ident" => &["xid", "ascii"],
            unknown => {
                let headline = format!("there is no preset called `{unknown}`");
                self.report("LM-F005", kind.text_range(), headline, PRESETS);
                return None;
            }
        };
        let mut chosen = HashSet::new();
        let mut interpolation = None;
        for option in &options {
            if option.is("Interpolate") {
                if kind.text() == "string" {
                    let delimiters = self.strings(option);
                    if let [open, close] = delimiters.as_slice() {
                        interpolation = Some((open.clone(), close.clone(), text(option.ident())));
                    }
                } else {
                    let first = option.tokens().next().map(|token| token.text_range());
                    let headline = format!("preset `{}` cannot have holes", kind.text());
                    self.report(
                        "LM-F006",
                        first.unwrap_or_else(|| option.range()),
                        headline,
                        "use `interpolate` only in the `string` preset",
                    );
                }
                continue;
            }
            let Some(word) = option.ident() else { continue };
            if allowed.contains(&word.text()) {
                chosen.insert(word.text().to_owned());
                continue;
            }
            let listed = match allowed.split_last() {
                None => "remove the options, because it has none".to_owned(),
                Some((last, [])) => format!("use `{last}`"),
                Some((last, rest)) => {
                    let rest: Vec<_> = rest.iter().map(|o| format!("`{o}`")).collect();
                    format!("use {} or `{last}`", rest.join(", "))
                }
            };
            let headline = format!("preset `{}` has no option `{}`", kind.text(), word.text());
            self.report("LM-F006", word.text_range(), headline, &listed);
        }
        let on = |option: &str| chosen.contains(option);
        Some(match kind.text() {
            "int" => Token::preset(
                name,
                Preset::Int(
                    IntPreset::default()
                        .hex(on("hex"))
                        .binary(on("bin"))
                        .octal(on("oct"))
                        .underscores(on("underscores")),
                ),
            ),
            "float" => Token::preset(
                name,
                Preset::Float(
                    FloatPreset::default()
                        .exponent(on("exponent"))
                        .underscores(on("underscores")),
                ),
            ),
            "char" => Token::preset(name, Preset::Char),
            "ident" => {
                let style = if on("ascii") {
                    IdentStyle::Ascii
                } else {
                    IdentStyle::Xid
                };
                Token::preset(name, Preset::Ident(style))
            }
            _ => {
                let quotes = StringPreset::default()
                    .double(on("double"))
                    .single(on("single"))
                    .raw(on("raw"));
                match interpolation {
                    Some((open, close, content)) => {
                        Token::interpolated(name, quotes, open, close, content)
                    }
                    None => Token::preset(name, Preset::String(quotes)),
                }
            }
        })
    }

    fn tokens(&mut self, block: &Node) {
        for item in block.children() {
            match item.name() {
                "KeywordList" => {
                    let words: Vec<String> = item.idents().map(|t| t.text().to_owned()).collect();
                    self.with(|builder| builder.keywords(words));
                }
                "ContextualList" => {
                    let words: Vec<String> = item.idents().map(|t| t.text().to_owned()).collect();
                    self.with(|builder| builder.contextual(words));
                }
                kind @ ("TokenDef" | "SkipToken") => {
                    let Some(name) = item.ident() else { continue };
                    self.lower_name("token", &name, true);
                    let Some(value) = item
                        .child("RegexValue")
                        .or_else(|| item.child("PresetValue"))
                    else {
                        continue;
                    };
                    let skip = kind == "SkipToken";
                    let token = if value.is("RegexValue") {
                        let written = text(value.all("regex").next());
                        let pattern = written
                            .strip_prefix('/')
                            .and_then(|rest| rest.strip_suffix('/'))
                            .unwrap_or(&written)
                            .replace("\\/", "/");
                        Some(if skip {
                            Token::skip(name.text(), pattern)
                        } else {
                            Token::regex(name.text(), pattern)
                        })
                    } else {
                        self.preset(name.text(), &value)
                            .map(|token| if skip { token.skipped() } else { token })
                    };
                    if let Some(token) = token {
                        let (file, range) = (self.file, name.text_range());
                        self.with(|builder| builder.token(token).locate(name.text(), file, range));
                    }
                }
                _ => {}
            }
        }
    }

    fn settings(&mut self, block: &Node) {
        for setting in block.children() {
            match setting.name() {
                "Extensions" => {
                    let extensions = self.strings(&setting);
                    self.extensions.extend(extensions.clone());
                    self.with(|builder| builder.extensions(extensions));
                }
                "CommentsDecl" => {
                    for comment in setting.children() {
                        let delimiters = self.strings(&comment);
                        let nested = comment.has("nested");
                        self.with(|builder| match (comment.name(), delimiters.as_slice()) {
                            ("LineComment", [open]) => builder.line_comment(open),
                            ("DocComment", [open]) => builder.doc_comment(open),
                            ("BlockComment", [open, close]) if nested => {
                                builder.nested_block_comment(open, close)
                            }
                            ("BlockComment", [open, close]) => builder.block_comment(open, close),
                            _ => builder,
                        });
                    }
                }
                "TerminatorsDecl" => {
                    let mut extra = Vec::new();
                    for newline in setting.children() {
                        extra.extend(self.strings(&newline));
                    }
                    self.with(|builder| builder.terminators(extra));
                }
                "ContinueInside" => {
                    let pairs: Vec<_> = setting
                        .children()
                        .filter_map(|pair| match self.strings(&pair).as_slice() {
                            [open, close] => Some((open.clone(), close.clone())),
                            _ => None,
                        })
                        .collect();
                    self.with(|builder| builder.continue_inside(pairs));
                }
                _ => {}
            }
        }
    }

    fn external(&mut self, node: &Node) {
        let (file, externals) = (self.file, self.options.externals);
        if let Some(declared) = node.child("ExternToken") {
            let Some(name) = declared.ident() else { return };
            self.type_name("token", &name);
            let word = name.text().to_owned();
            let token = match externals.extern_token(&word) {
                Some(token) => token,
                None => {
                    if !self.options.generic {
                        let headline = format!("extern token `{word}` has no Rust code");
                        self.report(
                            "LM-F009",
                            name.text_range(),
                            headline,
                            &format!("give its code with `Source::extern_token(\"{word}\", ...)`"),
                        );
                    }
                    Token::external(word.clone(), |_| None)
                }
            };
            let range = name.text_range();
            self.with(|builder| builder.token(token).locate(word, file, range));
            return;
        }
        let Some(declared) = node.child("ExternRule") else {
            return;
        };
        let Some(name) = declared.ident() else { return };
        self.type_name("rule", &name);
        let word = name.text().to_owned();
        let rule = match externals.extern_rule(&word) {
            Some(rule) => rule,
            None => {
                if !self.options.generic {
                    let headline = format!("extern rule `{word}` has no Rust code");
                    self.report(
                        "LM-F009",
                        name.text_range(),
                        headline,
                        &format!("give its code with `Source::extern_rule(\"{word}\", ...)`"),
                    );
                }
                let message = format!("`{word}` is parsed by Rust code that is not loaded");
                ExternRule::new(word.clone(), move |p: &mut Parser<'_>| {
                    // Without its code, one error stands for the whole rule.
                    let node = p.start();
                    p.error("LM-F012", message.clone());
                    p.bump();
                    node.complete(p, SyntaxKind::ERROR);
                })
            }
        };
        // A result that no rule or expression names is a node the code makes.
        let produces: Vec<String> = declared
            .all("ident")
            .nth(1)
            .map(|kind| kind.text().to_owned())
            .filter(|kind| !self.rules.contains(kind))
            .into_iter()
            .collect();
        let first = self.strings(&declared);
        let rule = rule.first(first).produces(produces);
        let range = name.text_range();
        self.with(|builder| builder.extern_rule(rule).locate(word, file, range));
    }

    fn path(&self, node: &Node) -> Path {
        Path {
            parts: node.idents().map(|t| t.text().to_owned()).collect(),
            place: place(self.file, node.range()),
        }
    }

    fn bindings(&mut self, block: &Node) {
        for statement in block.children() {
            let here = place(self.file, statement.range());
            if statement.is("ScopeStatement") {
                let nodes = statement.idents().map(|t| t.text().to_owned()).collect();
                self.bindings
                    .rules
                    .push(Binding::Scope { nodes, place: here });
                continue;
            }
            let targets: Vec<Path> = statement
                .children()
                .filter(|child| child.is("Path"))
                .map(|child| self.path(&child))
                .collect();
            let Some(verb) = statement.children().find(|child| !child.is("Path")) else {
                continue;
            };
            let namespace = text(verb.ident());
            let binding = match verb.name() {
                "Defines" => Binding::Defines {
                    targets,
                    namespace,
                    modifiers: verb
                        .children()
                        .map(|modifier| {
                            let words: Vec<_> =
                                modifier.tokens().map(|t| t.text().to_owned()).collect();
                            words.join(" ")
                        })
                        .collect(),
                    place: here,
                },
                "Refers" => match verb.child("Path") {
                    Some(of) => Binding::Member {
                        targets,
                        of: self.path(&of),
                        place: here,
                    },
                    None => Binding::Refers {
                        targets,
                        namespace,
                        place: here,
                    },
                },
                "Imports" => Binding::Imports {
                    targets,
                    namespace,
                    from: verb.child("Path").map_or_else(
                        || Path {
                            parts: Vec::new(),
                            place: here,
                        },
                        |path| self.path(&path),
                    ),
                    place: here,
                },
                _ => Binding::Includes {
                    targets,
                    of: verb.child("Path").map_or_else(
                        || Path {
                            parts: Vec::new(),
                            place: here,
                        },
                        |path| self.path(&path),
                    ),
                    place: here,
                },
            };
            self.bindings.rules.push(binding);
        }
    }

    fn editor(&mut self, block: &Node) {
        for statement in block.children() {
            let here = place(self.file, statement.range());
            let words: Vec<String> = statement.idents().map(|t| t.text().to_owned()).collect();
            let rule = match (statement.name(), words.as_slice()) {
                ("SymbolStatement", [node, kind, name]) => EditorRule::Symbol {
                    node: node.clone(),
                    kind: kind.clone(),
                    name: name.clone(),
                    place: here,
                },
                ("HighlightStatement", [node, by]) => EditorRule::Highlight {
                    node: node.clone(),
                    by: by.clone(),
                    place: here,
                },
                ("FoldStatement", _) => EditorRule::Fold {
                    nodes: words,
                    place: here,
                },
                _ => continue,
            };
            self.editor.rules.push(rule);
        }
    }

    /// The version line: required, and one this release reads.
    fn header(&mut self, unit: &Unit) {
        let Some(header) = unit.root.child("Header") else {
            let end = TextSize::from(u32::from(unit.root.range().end()).min(1));
            let range = TextRange::new(TextSize::from(0), end);
            self.report(
                "LM-F001",
                range,
                "the file does not start with its format version".to_owned(),
                &format!("write `langmeld {VERSION}` on the first line"),
            );
            return;
        };
        let Some(version) = header.all("number").next() else {
            return;
        };
        if version.text() != VERSION {
            let headline = format!(
                "the file is written for format version {}, which this release does not read",
                version.text()
            );
            self.report(
                "LM-F002",
                version.text_range(),
                headline,
                &format!(
                    "write `langmeld {VERSION}`, or use a release that reads version {}",
                    version.text()
                ),
            );
        }
    }
}

use langmeld_syntax::TextSize;

/// The rules and expressions a file declares.
fn declared_rules(root: &Node) -> impl Iterator<Item = String> {
    root.children()
        .filter(|item| item.is("RuleDecl") || item.is("ExprDecl") || item.is("ExternDecl"))
        .flat_map(|item| {
            let own = item.child("ExternRule").unwrap_or(item);
            own.ident()
        })
        .map(|name| name.text().to_owned())
}

/// Every name written in the files, with where, for renames.
fn written_names(units: &[Unit]) -> HashMap<String, Vec<(FileId, TextRange)>> {
    let mut names: HashMap<String, Vec<(FileId, TextRange)>> = HashMap::new();
    for unit in units {
        for element in unit.root.0.descendants_with_tokens() {
            if let Some(token) = element.into_token()
                && Some(token.kind()) == crate::meta::language().kind("ident")
            {
                names
                    .entry(token.text().to_owned())
                    .or_default()
                    .push((unit.file.id(), token.text_range()));
            }
        }
    }
    names
}

/// Declares everything the files hold, the first file's first rule being the one a
/// whole file is parsed with.
pub(crate) fn lower(units: &[Unit], options: &Options<'_>, fallback_name: &str) -> Lowered {
    let language = units
        .iter()
        .find_map(|unit| unit.root.child("LanguageDecl"))
        .and_then(|block| block.ident());
    let name = language
        .as_ref()
        .map_or_else(|| fallback_name.to_owned(), |t| t.text().to_owned());
    let mut lowering = Lowering {
        options,
        builder: Some(langmeld_syntax::Language::builder(name)),
        file: FileId::new(0),
        rules: units
            .iter()
            .flat_map(|unit| declared_rules(&unit.root))
            .collect(),
        names: written_names(units),
        extensions: Vec::new(),
        bindings: BindRules::default(),
        editor: Editor::default(),
        problems: Vec::new(),
    };
    let mut named = false;
    let mut root_rule = None;
    for unit in units {
        lowering.file = unit.file.id();
        lowering.header(unit);
        for item in unit.root.children() {
            match item.name() {
                "LanguageDecl" => {
                    if named {
                        lowering.report(
                            "LM-F013",
                            item.range(),
                            "the language is already described by another `language` block"
                                .to_owned(),
                            "keep one `language` block, and move this one's settings into it",
                        );
                    } else if let Some(name) = item.ident() {
                        let (file, range) = (lowering.file, name.text_range());
                        let word = name.text().to_owned();
                        lowering.with(|builder| builder.locate(word, file, range));
                    }
                    named = true;
                    lowering.settings(&item);
                }
                "TokensDecl" => lowering.tokens(&item),
                "RuleDecl" => {
                    let Some(name) = item.ident() else { continue };
                    lowering.type_name("rule", &name);
                    let body = lowering.pattern_of(&item, "Alternation");
                    let (file, range) = (lowering.file, name.text_range());
                    let word = name.text().to_owned();
                    root_rule.get_or_insert_with(|| word.clone());
                    lowering
                        .with(|builder| builder.rule(word.clone(), body).locate(word, file, range));
                    if let Some(recover) = item.child("Recover") {
                        let literals = lowering.strings(&recover);
                        let word = name.text().to_owned();
                        lowering.with(|builder| builder.recover(word, literals));
                    }
                }
                "ExprDecl" => lowering.expression(&item),
                "ExternDecl" => lowering.external(&item),
                "BindDecl" => lowering.bindings(&item),
                "EditorDecl" => lowering.editor(&item),
                _ => {}
            }
        }
    }
    if !named && let Some(unit) = units.first() {
        let range = TextRange::empty(TextSize::from(0));
        lowering.file = unit.file.id();
        lowering.report(
            "LM-F014",
            range,
            "the file does not say what language it describes".to_owned(),
            "add a block such as `language Name { extensions \"ext\" }`",
        );
    }
    let root = root_rule;
    lowering.with(|builder| match root {
        Some(root) => builder.root(root),
        None => builder,
    });
    Lowered {
        builder: lowering
            .builder
            .take()
            .expect("the builder is put back after every step"),
        extensions: lowering.extensions,
        bindings: lowering.bindings,
        editor: lowering.editor,
        problems: lowering.problems,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_converted_to_the_convention() {
        assert_eq!(pascal_case("fn_decl"), "FnDecl");
        assert_eq!(pascal_case("expr"), "Expr");
        assert_eq!(snake_case("FnDecl"), "fn_decl");
        assert_eq!(snake_case("XMLNode"), "xml_node");
        assert_eq!(snake_case("myValue2"), "my_value2");
        assert!(is_pascal("LetStmt") && !is_pascal("let_stmt") && !is_pascal("Let_Stmt"));
        assert!(is_snake("then") && is_snake("x_1") && !is_snake("Then") && !is_snake("a-b"));
    }
}
