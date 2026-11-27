//! Turns declarations into a [`Language`], in named steps: check the names, collect
//! the literals, number the kinds, build the lexer, lower the rules to parts, have
//! the parts checked and their decisions tabled, and assemble the result.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::{
    Diagnostic, FileId, Interpolation, Lexer, Preset, Severity, StringParts, SyntaxKind,
    Terminators, TextRange, TextSize, TokenSpec,
};

use super::analyze::{self, Input, Output};
use super::builder::{Comment, Definition, ExprSpec, LanguageBuilder, Operator, Token, TokenShape};
use super::ir::{
    Callee, Continuation, Expr, Extern, KindSet, Node, NodeId, Operation, Rule, Start, Symbol,
    Table,
};
use super::pattern::{Shape, r};
use super::problem::{error, to_fix, undefined, warning};
use super::{Language, Pattern, recovery, schema_of};

/// A language cannot have more kinds than this; the highest numbers are reserved.
const MOST_KINDS: usize = 60_000;

/// Gives problems the place where their subject was written, if it was recorded.
fn relocate(builder: &LanguageBuilder, problems: &mut [Diagnostic]) {
    let place = |name: &str| {
        builder
            .places
            .iter()
            .rev()
            .find(|(named, _)| named == name)
            .map(|(_, place)| *place)
    };
    for problem in problems {
        if problem.range != TextRange::empty(TextSize::new(0)) || problem.file != FileId::new(0) {
            continue;
        }
        let found = problem
            .about
            .as_deref()
            .and_then(place)
            .or_else(|| place(&builder.name));
        if let Some((file, range)) = found {
            problem.file = file;
            problem.range = range;
        }
    }
}

/// An interpolated string is a rule over the parts the lexer cuts it into.
fn desugar(mut builder: LanguageBuilder) -> LanguageBuilder {
    let mut tokens = Vec::new();
    for token in std::mem::take(&mut builder.tokens) {
        let TokenShape::Interpolated {
            quotes,
            open,
            close,
            content,
        } = token.shape
        else {
            tokens.push(token);
            continue;
        };
        let names: Vec<String> = ["start", "text", "end", "hole open", "hole close"]
            .iter()
            .map(|part| format!("{} {part}", token.name))
            .collect();
        let shape = TokenShape::Strings {
            quotes,
            open,
            close,
            parts: names.clone(),
        };
        for (index, name) in names.iter().enumerate() {
            let shape = if index == 0 {
                shape.clone()
            } else {
                TokenShape::Part
            };
            tokens.push(Token {
                name: name.clone(),
                shape,
                trivia: false,
                identifier: false,
            });
        }
        let [start, text, end, hole_open, hole_close] =
            <[String; 5]>::try_from(names).expect("five parts were named");
        let hole = super::pattern::seq([r(hole_open), r(content), r(hole_close)]);
        let body = super::pattern::seq([
            r(start),
            super::pattern::many(super::pattern::choice([r(text), hole])),
            // A string the lexer found no end for has been reported by the lexer.
            super::pattern::opt(r(end)),
        ]);
        builder.definitions.push(Definition::Rule(token.name, body));
    }
    builder.tokens = tokens;
    builder
}

pub(super) fn compile(builder: LanguageBuilder) -> Result<Language, Vec<Diagnostic>> {
    let builder = desugar(builder);
    if builder.definitions.is_empty() {
        let message = "the language has no rules";
        return Err(vec![to_fix(
            error("LM-G012", message),
            "declare at least one rule",
        )]);
    }
    let mut problems = Vec::new();
    let declared = declare(&builder, &mut problems);
    let literals = collect_literals(&builder, &declared);
    let mut vocabulary = Vocabulary::assign(&builder, &declared, literals);
    problems.extend(contextual_conflicts(&builder));
    let terminators = vocabulary.terminators(&builder, &declared);
    let lexer = build_lexer(&builder, &vocabulary, terminators.clone(), &mut problems);

    let numbering = Numbering::of(&declared, builder.root.as_deref());
    let mut lowering = Lowering::new(&declared, &vocabulary, &numbering, terminators.as_ref());
    let lowered =
        lowering.lower_definitions(&builder, &declared, numbering.root_name, &mut vocabulary);
    let resolved = lowering.errors.is_empty();
    problems.append(&mut lowering.errors);
    if numbering.root.is_none() {
        problems.push(undefined(
            format!(
                "`{}` is set as the root rule, but nothing is declared with that name",
                numbering.root_name
            ),
            numbering.root_name,
            &lowering.known,
        ));
    }
    if vocabulary.kinds.names.len() > MOST_KINDS {
        let message = format!("the language has more than {MOST_KINDS} kinds");
        problems.push(to_fix(
            error("LM-G014", message),
            "use fewer tokens and rules",
        ));
        // Their numbers are not distinct, so nothing further can be checked.
        relocate(&builder, &mut problems);
        return Err(problems);
    }

    let mut nodes = std::mem::take(&mut lowering.nodes);
    let (rules, mut exprs, externs) = (lowered.rules, lowered.exprs, lowered.externs);
    let mut warnings = Vec::new();
    for rule in externs.iter().filter(|rule| rule.first.is_empty()) {
        warnings.push(
            to_fix(
                warning(
                    "LM-G015",
                    format!(
                        "the custom rule `{}` says nothing about how it starts, so it may start \
                     with anything",
                        rule.name
                    ),
                ),
                "list the literals or tokens it can start with using `first`, so that choices \
             around it can be checked",
            )
            .about(&rule.name),
        );
    }
    if let Some(root) = numbering.root {
        let closers: Vec<SyntaxKind> = terminators
            .iter()
            .flat_map(|t| t.blocks.iter().map(|&(_, close)| close))
            .collect();
        let identifiers = vocabulary.identifiers(&declared);
        let input = Input {
            nodes: &nodes,
            rules: &rules,
            exprs: &exprs,
            externs: &externs,
            root,
            newline: vocabulary.lines.map(|(newline, _)| newline),
            terminators: &lowering.extra_terminators,
            closers: &closers,
            contextual: &vocabulary.contextual,
            identifiers: &identifiers,
            names: &vocabulary.kinds.names,
        };
        let output = analyze::analyze(&input, !resolved);
        // Custom code may call any rule, so with it none can be called unused.
        if externs.is_empty() {
            warnings.extend(unreachable_rules(&input));
        }
        problems.extend(output.errors.iter().cloned());
        let recovery = recovery::compute(
            &input,
            &output.follow_rule,
            &output.follow_expr,
            &lowered.named,
        );
        for (index, table) in output.expr_tables.iter().enumerate() {
            exprs[index].table = table.clone();
        }
        apply(&mut nodes, output, &recovery);
    }

    relocate(&builder, &mut problems);
    relocate(&builder, &mut warnings);
    let schema = schema_of::infer(&declared.definitions, &rules);
    let failed = problems.iter().any(|e| e.severity == Severity::Error);
    match (lexer, numbering.root) {
        (Some(lexer), Some(root)) if !failed => Ok(assemble(Parts {
            builder: &builder,
            lexer,
            nodes,
            rules,
            exprs,
            externs,
            root,
            vocabulary,
            lowering,
            lowered_kinds: lowered.node_kinds,
            schema,
            warnings,
        })),
        _ => {
            problems.extend(warnings);
            Err(problems)
        }
    }
}

// Step 1: names.

#[derive(Default)]
struct Declared<'a> {
    tokens: Vec<&'a Token>,
    definitions: Vec<&'a Definition>,
    /// Every name that something declares.
    names: HashSet<&'a str>,
}

fn declared_twice(name: &str) -> Diagnostic {
    to_fix(
        error("LM-G011", format!("`{name}` is declared twice")),
        "rename one of the two",
    )
    .about(name)
}

/// Names must be unique, whatever they name.
fn declare<'a>(builder: &'a LanguageBuilder, problems: &mut Vec<Diagnostic>) -> Declared<'a> {
    let mut declared = Declared::default();
    for token in &builder.tokens {
        if declared.names.insert(token.name.as_str()) {
            declared.tokens.push(token);
        } else {
            problems.push(declared_twice(&token.name));
        }
    }
    for definition in &builder.definitions {
        if declared.names.insert(definition.name()) {
            declared.definitions.push(definition);
        } else {
            problems.push(declared_twice(definition.name()));
        }
    }
    for definition in &declared.definitions {
        for name in node_names(definition) {
            if !declared.names.insert(name) {
                problems.push(declared_twice(name));
            }
        }
    }
    problems.sort_by(|a, b| a.message.cmp(&b.message));
    problems.dedup();
    declared
}

fn contextual_conflicts(builder: &LanguageBuilder) -> Vec<Diagnostic> {
    builder
        .contextual
        .iter()
        .filter(|word| builder.keywords.contains(word))
        .map(|word| {
            to_fix(
                error(
                    "LM-G011",
                    format!("`{word}` is declared both reserved and contextual"),
                ),
                "keep one of the two",
            )
            .about(word)
        })
        .collect()
}

fn patterns(definition: &Definition) -> Vec<&Pattern> {
    match definition {
        Definition::Rule(_, body) => vec![body],
        Definition::Expr(expr) => expr
            .atoms
            .iter()
            .map(|(_, body)| body)
            .chain(expr.operators.iter().filter_map(|operator| match operator {
                Operator::Postfix { body, .. } => Some(body),
                Operator::Prefix { .. } | Operator::Infix { .. } => None,
            }))
            .collect(),
        Definition::Extern(_) => Vec::new(),
    }
}

/// The names of the nodes a definition makes besides its own.
fn node_names(definition: &Definition) -> Vec<&str> {
    match definition {
        Definition::Rule(..) => Vec::new(),
        Definition::Expr(expr) => expr
            .atoms
            .iter()
            .map(|(name, _)| name.as_str())
            .chain(expr.operators.iter().map(|operator| match operator {
                Operator::Prefix { name, .. }
                | Operator::Infix { name, .. }
                | Operator::Postfix { name, .. } => name.as_str(),
            }))
            .collect(),
        Definition::Extern(rule) => rule.produces.iter().map(String::as_str).collect(),
    }
}

// Step 2: literals.

/// Literal texts in the order the grammar first mentions them.
#[derive(Default)]
struct Literals {
    ordered: Vec<String>,
    seen: HashSet<String>,
}

impl Literals {
    fn add(&mut self, text: &str) {
        if self.seen.insert(text.to_owned()) {
            self.ordered.push(text.to_owned());
        }
    }

    fn collect(&mut self, pattern: &Pattern) {
        match &pattern.0 {
            Shape::Literal(text) => self.add(text),
            Shape::Reference(_) => {}
            Shape::Sequence(parts) | Shape::Choice(parts) => {
                for part in parts {
                    self.collect(part);
                }
            }
            Shape::Separated {
                item, separator, ..
            } => {
                self.collect(item);
                self.add(separator);
            }
            Shape::Optional(part)
            | Shape::Many(part)
            | Shape::Many1(part)
            | Shape::Terminated(part)
            | Shape::Field(_, part)
            | Shape::Look(part)
            | Shape::Not(part)
            | Shape::Attempt(part) => self.collect(part),
        }
    }
}

fn uses_terminators(pattern: &Pattern) -> bool {
    match &pattern.0 {
        Shape::Terminated(_) => true,
        Shape::Literal(_) | Shape::Reference(_) => false,
        Shape::Sequence(parts) | Shape::Choice(parts) => parts.iter().any(uses_terminators),
        Shape::Separated { item, .. } => uses_terminators(item),
        Shape::Optional(part)
        | Shape::Many(part)
        | Shape::Many1(part)
        | Shape::Field(_, part)
        | Shape::Look(part)
        | Shape::Not(part)
        | Shape::Attempt(part) => uses_terminators(part),
    }
}

fn collect_literals(builder: &LanguageBuilder, declared: &Declared<'_>) -> Literals {
    let mut literals = Literals::default();
    for definition in &declared.definitions {
        for pattern in patterns(definition) {
            literals.collect(pattern);
        }
        match definition {
            Definition::Expr(expr) => {
                for operator in &expr.operators {
                    if let Operator::Prefix { symbols, .. } | Operator::Infix { symbols, .. } =
                        operator
                    {
                        for symbol in symbols {
                            literals.add(symbol);
                        }
                    }
                }
            }
            Definition::Extern(rule) => {
                for text in rule.first.iter().chain(&rule.uses) {
                    if !declared.tokens.iter().any(|token| token.name == *text) {
                        literals.add(text);
                    }
                }
            }
            Definition::Rule(..) => {}
        }
    }
    for text in builder.terminators.iter().flatten() {
        literals.add(text);
    }
    for (open, close) in builder.continue_inside.iter().flatten() {
        literals.add(open);
        literals.add(close);
    }
    for text in builder.recover.iter().flat_map(|(_, texts)| texts) {
        literals.add(text);
    }
    for word in builder.keywords.iter().chain(&builder.contextual) {
        literals.add(word);
    }
    literals
}

// Step 3: kinds.

#[derive(Default)]
struct Kinds {
    names: Vec<String>,
}

impl Kinds {
    fn add(&mut self, name: impl Into<String>) -> SyntaxKind {
        // `compile` refuses languages with more kinds than fit.
        let kind = SyntaxKind(self.names.len().min(MOST_KINDS) as u16);
        self.names.push(name.into());
        kind
    }
}

/// The kind of every token, literal and node.
struct Vocabulary<'a> {
    kinds: Kinds,
    /// Declared tokens.
    tokens: HashMap<&'a str, SyntaxKind>,
    comments: Vec<TokenSpec>,
    /// Literal texts that are tokens of their own, in the order they are mentioned.
    plain: Vec<String>,
    literals: HashMap<String, SyntaxKind>,
    /// Operators made of tokens that the lexer reads one at a time.
    glued: HashMap<String, Symbol>,
    contextual: HashMap<SyntaxKind, String>,
    /// The kinds of a line ending that terminates and of every other, if lines
    /// terminate at all.
    lines: Option<(SyntaxKind, SyntaxKind)>,
}

fn is_word(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c == '_' || unicode_ident::is_xid_start(c))
        && chars.all(unicode_ident::is_xid_continue)
}

impl<'a> Vocabulary<'a> {
    fn assign(builder: &LanguageBuilder, declared: &Declared<'a>, literals: Literals) -> Self {
        // `>>` and its like are read as separate tokens, so that `>` can close generic
        // lists; the parser joins them where an operator is meant.
        let has_closing_angle = literals.seen.contains(">");
        let (joined, mut plain): (Vec<_>, Vec<_>) = literals
            .ordered
            .iter()
            .cloned()
            .partition(|text| has_closing_angle && text.len() > 1 && text.starts_with('>'));
        for text in &joined {
            for part in text.chars() {
                let part = part.to_string();
                if !plain.contains(&part) {
                    plain.push(part);
                }
            }
        }

        let mut kinds = Kinds::default();
        let tokens: HashMap<&str, SyntaxKind> = declared
            .tokens
            .iter()
            .map(|token| (token.name.as_str(), kinds.add(&token.name)))
            .collect();
        let comments = builder
            .comments
            .iter()
            .map(|comment| match comment {
                Comment::Line(open) => TokenSpec::line_comment(kinds.add("comment"), open.clone()),
                Comment::Doc(open) => {
                    TokenSpec::line_comment(kinds.add("doc comment"), open.clone()).doc()
                }
                Comment::Block {
                    open,
                    close,
                    nested,
                } => {
                    let spec = TokenSpec::block_comment(
                        kinds.add("block comment"),
                        open.clone(),
                        close.clone(),
                    );
                    if *nested { spec.nested() } else { spec }
                }
            })
            .collect();
        let literal_kinds: HashMap<String, SyntaxKind> = plain
            .iter()
            .map(|text| (text.clone(), kinds.add(text)))
            .collect();
        let glued = joined
            .iter()
            .map(|text| {
                let parts = text
                    .chars()
                    .filter_map(|c| literal_kinds.get(&c.to_string()).copied())
                    .collect();
                let kind = kinds.add(text);
                (text.clone(), Symbol { parts, kind })
            })
            .collect();
        let wants_lines = builder.terminators.is_some()
            || declared
                .definitions
                .iter()
                .flat_map(|definition| patterns(definition))
                .any(uses_terminators);
        let lines = wants_lines.then(|| (kinds.add("end of line"), kinds.add("line break")));
        let contextual = builder
            .contextual
            .iter()
            .filter_map(|word| Some((*literal_kinds.get(word)?, word.clone())))
            .collect();
        Self {
            kinds,
            tokens,
            comments,
            plain,
            literals: literal_kinds,
            glued,
            contextual,
            lines,
        }
    }

    /// What the lexer needs to know about lines, if they terminate anything.
    fn terminators(
        &self,
        builder: &LanguageBuilder,
        declared: &Declared<'_>,
    ) -> Option<Terminators> {
        let (newline, line_break) = self.lines?;
        let mut terminators = Terminators::new(newline, line_break);
        let pair =
            |open: &str, close: &str| Some((*self.literals.get(open)?, *self.literals.get(close)?));
        terminators.brackets = match &builder.continue_inside {
            Some(pairs) => pairs
                .iter()
                .filter_map(|(open, close)| pair(open, close))
                .collect(),
            None => [pair("(", ")"), pair("[", "]")]
                .into_iter()
                .flatten()
                .collect(),
        };
        terminators.blocks = pair("{", "}").into_iter().collect();
        // A line ending in a binary operator or a comma continues.
        let operators = declared
            .definitions
            .iter()
            .filter_map(|definition| match definition {
                Definition::Expr(expr) => Some(&expr.operators),
                _ => None,
            })
            .flatten()
            .filter_map(|operator| match operator {
                Operator::Infix { symbols, .. } => Some(symbols),
                _ => None,
            })
            .flatten();
        terminators.continue_after = operators
            .map(String::as_str)
            .chain([","])
            .filter_map(|text| {
                self.literals
                    .get(text)
                    .copied()
                    .or_else(|| self.glued.get(text)?.parts.last().copied())
            })
            .collect();
        Some(terminators)
    }

    fn identifiers(&self, declared: &Declared<'_>) -> Vec<SyntaxKind> {
        declared
            .tokens
            .iter()
            .filter(|token| token.identifier)
            .filter_map(|token| self.tokens.get(token.name.as_str()).copied())
            .collect()
    }
}

// Step 4: the lexer.

fn token_specs(builder: &LanguageBuilder, vocabulary: &Vocabulary<'_>) -> Vec<TokenSpec> {
    let described = |name: &str| {
        builder
            .descriptions
            .iter()
            .rev()
            .find(|(described, _)| described == name)
            .map_or_else(|| name.to_owned(), |(_, text)| text.clone())
    };
    let mut specs: Vec<TokenSpec> = builder
        .tokens
        .iter()
        .filter_map(|token| {
            let &kind = vocabulary.tokens.get(token.name.as_str())?;
            let name = described(&token.name);
            let mut spec = match &token.shape {
                TokenShape::Strings {
                    quotes,
                    open,
                    close,
                    parts,
                } => {
                    let kind_of = |index: usize| vocabulary.tokens.get(parts[index].as_str());
                    let (&start, &text, &end, &hole_open, &hole_close) = (
                        kind_of(0)?,
                        kind_of(1)?,
                        kind_of(2)?,
                        kind_of(3)?,
                        kind_of(4)?,
                    );
                    let parts = StringParts {
                        start,
                        text,
                        end,
                        hole_open,
                        hole_close,
                    };
                    let quotes = quotes.clone().interpolation(Interpolation::new(
                        open.clone(),
                        close.clone(),
                        parts,
                    ));
                    TokenSpec::preset(kind, name, Preset::String(quotes))
                }
                TokenShape::Interpolated { .. } | TokenShape::Part => return None,
                TokenShape::Regex(pattern) => TokenSpec::regex(kind, name, pattern.clone()),
                TokenShape::Preset(preset) => TokenSpec::preset(kind, name, preset.clone()),
                TokenShape::External(lex) => {
                    let lex = lex.clone();
                    TokenSpec::external(kind, name, move |cursor| lex(cursor))
                }
            };
            if token.trivia {
                spec = spec.skip();
            }
            if token.identifier {
                spec = spec.identifier();
            }
            Some(spec)
        })
        .collect();
    specs.extend(vocabulary.comments.iter().cloned());
    for text in &vocabulary.plain {
        let Some(&kind) = vocabulary.literals.get(text) else {
            continue;
        };
        specs.push(if vocabulary.contextual.contains_key(&kind) {
            TokenSpec::contextual(kind, text.clone())
        } else if is_word(text) {
            TokenSpec::keyword(kind, text.clone())
        } else {
            TokenSpec::punct(kind, text.clone())
        });
    }
    specs
}

fn build_lexer(
    builder: &LanguageBuilder,
    vocabulary: &Vocabulary<'_>,
    terminators: Option<Terminators>,
    problems: &mut Vec<Diagnostic>,
) -> Option<Lexer> {
    let mut lexer = Lexer::builder()
        .tokens(token_specs(builder, vocabulary))
        .limits(builder.lex_limits);
    if let Some(terminators) = terminators {
        lexer = lexer.terminators(terminators);
    }
    match lexer.build() {
        Ok(lexer) => Some(lexer),
        Err(found) => {
            problems.extend(found);
            None
        }
    }
}

// Step 5: numbering the definitions.

struct Numbering<'a> {
    callees: HashMap<&'a str, Callee>,
    root_name: &'a str,
    root: Option<Callee>,
}

impl<'a> Numbering<'a> {
    /// Definitions get their numbers before anything refers to them.
    fn of(declared: &Declared<'a>, root: Option<&'a str>) -> Self {
        let mut callees = HashMap::new();
        let (mut rules, mut exprs, mut externs) = (0, 0, 0);
        for definition in &declared.definitions {
            let (callee, count) = match definition {
                Definition::Rule(..) => (Callee::Rule(rules), &mut rules),
                Definition::Expr(_) => (Callee::Expr(exprs), &mut exprs),
                Definition::Extern(_) => (Callee::Extern(externs), &mut externs),
            };
            *count += 1;
            callees.insert(definition.name(), callee);
        }
        let root_name = root.unwrap_or_else(|| {
            declared
                .definitions
                .iter()
                .find(|definition| matches!(definition, Definition::Rule(..)))
                .or_else(|| declared.definitions.first())
                .map_or("", |definition| definition.name())
        });
        let root = callees.get(root_name).copied();
        Self {
            callees,
            root_name,
            root,
        }
    }
}

// Step 6: lowering patterns to parts.

struct Lowering<'a> {
    nodes: Vec<Node>,
    callees: HashMap<&'a str, Callee>,
    tokens: HashMap<&'a str, SyntaxKind>,
    literals: HashMap<String, SyntaxKind>,
    /// The kinds of `literals`.
    literal_set: HashSet<SyntaxKind>,
    glued: HashMap<String, Symbol>,
    contextual: HashMap<SyntaxKind, String>,
    /// The pairs of kinds between which a line ending ends only the statements written
    /// there, as the lexer is told.
    blocks: Vec<(SyntaxKind, SyntaxKind)>,
    /// Terminating literals besides the end of a line.
    extra_terminators: Vec<Symbol>,
    known: Vec<&'a str>,
    errors: Vec<Diagnostic>,
}

/// The definitions as parts.
struct Lowered {
    rules: Vec<Rule>,
    exprs: Vec<Expr>,
    externs: Vec<Extern>,
    node_kinds: HashMap<String, SyntaxKind>,
    /// By rule, the kinds the grammar names to resume at.
    named: Vec<KindSet>,
}

impl<'a> Lowering<'a> {
    fn new(
        declared: &Declared<'a>,
        vocabulary: &Vocabulary<'a>,
        numbering: &Numbering<'a>,
        terminators: Option<&Terminators>,
    ) -> Self {
        let mut known: Vec<&str> = declared.names.iter().copied().collect();
        known.sort_unstable();
        Self {
            nodes: Vec::new(),
            callees: numbering.callees.clone(),
            tokens: vocabulary.tokens.clone(),
            literals: vocabulary.literals.clone(),
            literal_set: vocabulary.literals.values().copied().collect(),
            glued: vocabulary.glued.clone(),
            contextual: vocabulary.contextual.clone(),
            blocks: terminators.map(|t| t.blocks.clone()).unwrap_or_default(),
            extra_terminators: Vec::new(),
            known,
            errors: Vec::new(),
        }
    }

    fn push(&mut self, node: Node) -> NodeId {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    fn symbol(&self, text: &str) -> Symbol {
        self.glued.get(text).cloned().unwrap_or_else(|| {
            let kind = self
                .literals
                .get(text)
                .copied()
                .unwrap_or(SyntaxKind::ERROR);
            Symbol {
                parts: vec![kind],
                kind,
            }
        })
    }

    /// The literal a part always starts with, if it does.
    fn leading_literal(&self, node: NodeId) -> Option<SyntaxKind> {
        match &self.nodes[node] {
            Node::Token(kind) if self.literal_set.contains(kind) => Some(*kind),
            Node::Contextual(kind) => Some(*kind),
            Node::Sequence(parts) => self.leading_literal(*parts.first()?),
            _ => None,
        }
    }

    fn undefined_name(&mut self, holder: &str, name: &str, place: Option<(FileId, TextRange)>) {
        let message = format!("`{holder}` uses `{name}`, which is not a rule or a token");
        let known = self.known.clone();
        let mut problem = undefined(message, name, &known).about(holder);
        if let Some((file, range)) = place {
            problem.file = file;
            problem.range = range;
        }
        self.errors.push(problem);
    }

    fn lower(&mut self, holder: &str, pattern: &Pattern) -> NodeId {
        match &pattern.0 {
            Shape::Literal(text) => {
                if let Some(symbol) = self.glued.get(text) {
                    let (parts, kind) = (symbol.parts.clone(), symbol.kind);
                    return self.push(Node::Glued { parts, kind });
                }
                let kind = self
                    .literals
                    .get(text)
                    .copied()
                    .unwrap_or(SyntaxKind::ERROR);
                if self.contextual.contains_key(&kind) {
                    self.push(Node::Contextual(kind))
                } else {
                    self.push(Node::Token(kind))
                }
            }
            Shape::Reference(name) => {
                let node = match self.callees.get(name.as_str()) {
                    Some(&Callee::Rule(index)) => Node::Rule(index),
                    Some(&Callee::Expr(index)) => Node::Expr(index),
                    Some(&Callee::Extern(index)) => Node::Extern(index),
                    None => match self.tokens.get(name.as_str()) {
                        Some(&kind) => Node::Token(kind),
                        None => {
                            self.undefined_name(holder, name, pattern.1);
                            Node::Missing
                        }
                    },
                };
                self.push(node)
            }
            Shape::Sequence(parts) => {
                let parts: Vec<_> = parts.iter().map(|part| self.lower(holder, part)).collect();
                self.note_followers(&parts);
                let parts = self.scope_blocks(&parts);
                self.push(Node::Sequence(parts))
            }
            Shape::Choice(alternatives) => self.lower_choice(holder, alternatives),
            Shape::Optional(part) => {
                let body = self.lower(holder, part);
                let literal = self.leading_literal(body);
                self.push(Node::Optional {
                    body,
                    table: Table::default(),
                    literal,
                })
            }
            Shape::Many(part) => {
                let body = self.lower(holder, part);
                self.push(Node::Many {
                    body,
                    table: Table::default(),
                    stop: KindSet::default(),
                })
            }
            Shape::Many1(part) => {
                let first = self.lower(holder, part);
                let rest = self.lower(holder, &super::pattern::many((**part).clone()));
                self.push(Node::Sequence(vec![first, rest]))
            }
            Shape::Separated {
                item,
                separator,
                strict,
            } => {
                let item = self.lower(holder, item);
                let separator = self.symbol(separator);
                self.push(Node::Separated {
                    item,
                    separator,
                    strict: *strict,
                    table: Table::default(),
                    follow: KindSet::default(),
                    outer: KindSet::default(),
                    resume: KindSet::default(),
                    follow_literals: Vec::new(),
                })
            }
            Shape::Field(_, part) => self.lower(holder, part),
            Shape::Terminated(part) => {
                let body = self.lower(holder, part);
                self.push(Node::Terminated(body))
            }
            Shape::Look(part) => {
                let body = self.lower(holder, part);
                self.push(Node::Look(body))
            }
            Shape::Not(part) => {
                let body = self.lower(holder, part);
                self.push(Node::Not(body))
            }
            Shape::Attempt(part) => {
                let body = self.lower(holder, part);
                self.push(Node::Attempt(body))
            }
        }
    }

    fn lower_choice(&mut self, holder: &str, alternatives: &[Pattern]) -> NodeId {
        let mut attempts = Vec::new();
        let parts: Vec<_> = alternatives
            .iter()
            .map(|alternative| {
                // Here an attempt means trying the alternative first, so it is not a
                // part of its own.
                match attempted(alternative) {
                    Some(body) => {
                        let part = self.lower(holder, body);
                        attempts.push(part);
                        part
                    }
                    None => self.lower(holder, alternative),
                }
            })
            .collect();
        // A message can list the literals if every other way starts with one.
        let tried: HashSet<NodeId> = attempts.iter().copied().collect();
        let literals: Option<Vec<_>> = parts
            .iter()
            .filter(|&part| !tried.contains(part))
            .map(|&part| match self.nodes[part] {
                Node::Look(_) => Some(None),
                _ => self.leading_literal(part).map(Some),
            })
            .collect();
        let literals = literals.into_iter().flatten().flatten().collect();
        self.push(Node::Choice {
            alternatives: parts,
            attempts,
            table: Table::default(),
            empty: None,
            literals,
        })
    }

    /// Tells each list in a sequence which literals can close it.
    fn note_followers(&mut self, parts: &[NodeId]) {
        for (position, &part) in parts.iter().enumerate() {
            if !matches!(self.nodes[part], Node::Separated { .. }) {
                continue;
            }
            let mut found = Vec::new();
            for &after in &parts[position + 1..] {
                match &self.nodes[after] {
                    Node::Optional { literal, .. } => found.extend(*literal),
                    _ => {
                        found.extend(self.leading_literal(after));
                        break;
                    }
                }
            }
            if let Node::Separated {
                follow_literals, ..
            } = &mut self.nodes[part]
            {
                *follow_literals = found;
            }
        }
    }

    /// The kind that closes the block `part` opens, if it opens one.
    fn closing(&self, part: NodeId) -> Option<(SyntaxKind, SyntaxKind)> {
        let Node::Token(kind) = self.nodes[part] else {
            return None;
        };
        self.blocks.iter().copied().find(|&(open, _)| open == kind)
    }

    fn is_token(&self, part: NodeId, wanted: SyntaxKind) -> bool {
        matches!(self.nodes[part], Node::Token(kind) if kind == wanted)
    }

    /// Wraps what is written between the opening and the closing of a block, as the
    /// lexer is told them, so that a line ending there ends only the statements
    /// written there. Without a closing, the rest of the sequence is inside.
    fn scope_blocks(&mut self, parts: &[NodeId]) -> Vec<NodeId> {
        let mut result = Vec::new();
        let mut at = 0;
        while at < parts.len() {
            let part = parts[at];
            result.push(part);
            at += 1;
            let Some((open, close)) = self.closing(part) else {
                continue;
            };
            let mut depth = 1;
            let mut end = at;
            while end < parts.len() {
                if self.is_token(parts[end], open) {
                    depth += 1;
                } else if self.is_token(parts[end], close) {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                end += 1;
            }
            let mut inside = self.scope_blocks(&parts[at..end]);
            let body = match inside.len() {
                0 => None,
                1 => inside.pop(),
                _ => Some(self.push(Node::Sequence(inside))),
            };
            if let Some(body) = body {
                result.push(self.push(Node::Block(body)));
            }
            at = end;
        }
        result
    }

    fn lower_definitions(
        &mut self,
        builder: &LanguageBuilder,
        declared: &Declared<'a>,
        root_name: &str,
        vocabulary: &mut Vocabulary<'a>,
    ) -> Lowered {
        self.extra_terminators = builder
            .terminators
            .iter()
            .flatten()
            .map(|text| self.symbol(text))
            .collect();
        let mut lowered = Lowered {
            rules: Vec::new(),
            exprs: Vec::new(),
            externs: Vec::new(),
            node_kinds: HashMap::new(),
            named: Vec::new(),
        };
        let description = |name: &str, otherwise: Option<&str>| {
            builder
                .descriptions
                .iter()
                .rev()
                .find(|(rule, _)| rule == name)
                .map_or_else(
                    || otherwise.map_or_else(|| describe(name), str::to_owned),
                    |(_, text)| text.clone(),
                )
        };
        for definition in &declared.definitions {
            match definition {
                Definition::Rule(name, body) => {
                    let transparent = name != root_name && is_transparent(body, &self.callees);
                    let kind = (!transparent).then(|| {
                        let kind = vocabulary.kinds.add(name);
                        lowered.node_kinds.insert(name.clone(), kind);
                        kind
                    });
                    let body = self.lower(name, body);
                    let mut named = KindSet::default();
                    for text in builder
                        .recover
                        .iter()
                        .filter(|(rule, _)| rule == name)
                        .flat_map(|(_, texts)| texts)
                    {
                        // A joined operator is resumed at by its first character.
                        if let Some(&kind) = self.symbol(text).parts.first() {
                            named.insert(kind);
                        }
                    }
                    lowered.named.push(named);
                    lowered.rules.push(Rule {
                        name: name.clone(),
                        kind,
                        body,
                        description: description(name, None),
                    });
                }
                Definition::Expr(spec) => {
                    let expr =
                        self.lower_expr(spec, vocabulary, &mut lowered, |name, otherwise| {
                            description(name, otherwise)
                        });
                    lowered.exprs.push(expr);
                }
                Definition::Extern(rule) => {
                    for name in &rule.produces {
                        let kind = vocabulary.kinds.add(name);
                        lowered.node_kinds.insert(name.clone(), kind);
                    }
                    let first = rule
                        .first
                        .iter()
                        .filter_map(|text| match vocabulary.tokens.get(text.as_str()) {
                            Some(&kind) => Some(kind),
                            // A joined operator starts with its first character.
                            None => self.symbol(text).parts.first().copied(),
                        })
                        .collect();
                    lowered.externs.push(Extern {
                        name: rule.name.clone(),
                        parse: rule.parse.clone(),
                        first,
                    });
                }
            }
        }
        for (rule, _) in &builder.recover {
            if !lowered.rules.iter().any(|declared| declared.name == *rule) {
                let message = format!("`recover` names `{rule}`, which is not a rule");
                self.errors
                    .push(undefined(message, rule, &self.known).about(rule));
            }
        }
        lowered
    }

    fn lower_expr(
        &mut self,
        spec: &ExprSpec,
        vocabulary: &mut Vocabulary<'a>,
        lowered: &mut Lowered,
        description: impl Fn(&str, Option<&str>) -> String,
    ) -> Expr {
        let mut node = |kinds: &mut Kinds, name: &str| {
            let kind = kinds.add(name);
            lowered.node_kinds.insert(name.to_owned(), kind);
            kind
        };
        let mut starts = Vec::new();
        let mut continuations = Vec::new();
        for (name, body) in &spec.atoms {
            let kind = node(&mut vocabulary.kinds, name);
            let body = self.lower(&spec.name, body);
            starts.push(Start::Atom { node: kind, body });
        }
        for operator in &spec.operators {
            match operator {
                Operator::Prefix {
                    name,
                    symbols,
                    precedence,
                } => {
                    let kind = node(&mut vocabulary.kinds, name);
                    starts.extend(symbols.iter().map(|text| Start::Prefix {
                        symbol: self.symbol(text),
                        node: kind,
                        precedence: *precedence,
                    }));
                }
                Operator::Infix {
                    name,
                    symbols,
                    precedence,
                    assoc,
                } => {
                    let kind = node(&mut vocabulary.kinds, name);
                    continuations.extend(symbols.iter().map(|text| Continuation {
                        symbol: self.symbol(text),
                        node: kind,
                        precedence: *precedence,
                        operation: Operation::Infix { assoc: *assoc },
                    }));
                }
                Operator::Postfix {
                    name,
                    body,
                    precedence,
                } => {
                    let kind = node(&mut vocabulary.kinds, name);
                    let Some(symbol) = leading_text(body).map(|text| self.symbol(text)) else {
                        let message = format!(
                            "in `{}`, the postfix operator `{name}` does not start with a literal",
                            spec.name
                        );
                        self.errors.push(
                            to_fix(
                                error("LM-G013", message),
                                "start it with the symbol that marks the operator",
                            )
                            .about(&spec.name),
                        );
                        continue;
                    };
                    let body = self.lower(&spec.name, body);
                    continuations.push(Continuation {
                        symbol,
                        node: kind,
                        precedence: *precedence,
                        operation: Operation::Postfix { body },
                    });
                }
            }
        }
        // The longest symbol is tried first, so that `>=` is not read as `>`.
        continuations.sort_by_key(|c| std::cmp::Reverse(c.symbol.parts.len()));
        let by_first = index_by_first(&continuations);
        Expr {
            name: spec.name.clone(),
            description: description(&spec.name, Some("an expression")),
            starts,
            table: Table::default(),
            continuations,
            by_first,
        }
    }
}

/// What an alternative written as an attempt, possibly labelled, tries.
fn attempted(pattern: &Pattern) -> Option<&Pattern> {
    match &pattern.0 {
        Shape::Attempt(body) => Some(body),
        Shape::Field(_, part) => attempted(part),
        _ => None,
    }
}

/// Whether a rule only chooses between other rules, and so needs no node.
fn is_transparent(body: &Pattern, callees: &HashMap<&str, Callee>) -> bool {
    match &body.0 {
        Shape::Choice(alternatives) => {
            alternatives.len() > 1
                && alternatives.iter().all(|alternative| {
                    matches!(&alternative.0, Shape::Reference(name)
                        if callees.contains_key(name.as_str()))
                })
        }
        _ => false,
    }
}

/// `ExprStmt` as `an expr stmt`.
fn describe(name: &str) -> String {
    let mut words = String::new();
    for (index, c) in name.chars().enumerate() {
        if c.is_uppercase() && index > 0 {
            words.push(' ');
        }
        words.extend(c.to_lowercase());
    }
    let article = if words.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    };
    format!("{article} {words}")
}

fn index_by_first(continuations: &[Continuation]) -> Vec<Vec<usize>> {
    let mut index: Vec<Vec<usize>> = Vec::new();
    for (position, continuation) in continuations.iter().enumerate() {
        if let Some(first) = continuation.symbol.parts.first() {
            let slot = usize::from(first.0);
            if index.len() <= slot {
                index.resize_with(slot + 1, Vec::new);
            }
            index[slot].push(position);
        }
    }
    index
}

/// The literal a pattern starts with.
fn leading_text(pattern: &Pattern) -> Option<&str> {
    match &pattern.0 {
        Shape::Literal(text) => Some(text),
        Shape::Sequence(parts) => leading_text(parts.first()?),
        Shape::Field(_, part) => leading_text(part),
        _ => None,
    }
}

// Step 7: what the analysis worked out, put into the parts.

/// Puts the decisions and recovery sets into the parts they concern.
fn apply(nodes: &mut [Node], mut output: Output, recovery: &recovery::Recovery) {
    let lookaheads: HashSet<NodeId> = nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| matches!(node, Node::Look(_) | Node::Not(_)))
        .map(|(index, _)| index)
        .collect();
    for (index, node) in nodes.iter_mut().enumerate() {
        let table_here = output.tables.remove(&index).unwrap_or_default();
        // Where reading can go on: what the rule around can go on with, what the
        // repetition starts with and what follows it.
        let mut around = recovery.of(index).cloned().unwrap_or_default();
        if let Some(follow) = output.after.get(&index) {
            around.union(follow);
        }
        match node {
            Node::Choice {
                alternatives,
                table,
                empty,
                ..
            } => {
                *table = table_here;
                *empty = alternatives
                    .iter()
                    .copied()
                    .find(|&part| output.nullable[part] && !lookaheads.contains(&part));
            }
            Node::Optional { table, .. } => *table = table_here,
            Node::Many { table, stop, .. } => {
                *table = table_here;
                *stop = around;
                if let Some(first) = output.starts.get(&index) {
                    stop.union(first);
                }
            }
            Node::Separated {
                table,
                follow,
                outer,
                resume,
                separator,
                ..
            } => {
                *table = table_here;
                if let Some(found) = output.after.get(&index) {
                    follow.union(found);
                }
                // A closing token may be missing, so what follows it is a place to go on.
                if let Some(beyond) = output.beyond.get(&index) {
                    around.union(beyond);
                }
                *outer = around;
                resume.union(outer);
                if let Some(first) = output.starts.get(&index) {
                    resume.union(first);
                }
                if let Some(&first) = separator.parts.first() {
                    resume.insert(first);
                }
            }
            _ => {}
        }
    }
}

fn unreachable_rules(input: &Input<'_>) -> Vec<Diagnostic> {
    let mut reached = BTreeSet::new();
    let mut pending = vec![input.root];
    while let Some(callee) = pending.pop() {
        if !reached.insert(callee) {
            continue;
        }
        let mut parts: Vec<NodeId> = match callee {
            Callee::Rule(rule) => vec![input.rules[rule].body],
            Callee::Expr(expr) => analyze::bodies(&input.exprs[expr]).collect(),
            Callee::Extern(_) => Vec::new(),
        };
        while let Some(part) = parts.pop() {
            match &input.nodes[part] {
                Node::Rule(rule) => pending.push(Callee::Rule(*rule)),
                Node::Expr(expr) => pending.push(Callee::Expr(*expr)),
                Node::Extern(index) => pending.push(Callee::Extern(*index)),
                node => analyze::children(node, &mut parts),
            }
        }
    }
    let rules = input
        .rules
        .iter()
        .enumerate()
        .map(|(index, rule)| (Callee::Rule(index), &rule.name));
    let exprs = input
        .exprs
        .iter()
        .enumerate()
        .map(|(index, expr)| (Callee::Expr(index), &expr.name));
    rules
        .chain(exprs)
        .filter(|(callee, _)| !reached.contains(callee))
        .map(|(_, name)| {
            to_fix(
                warning("LM-G006", format!("`{name}` is never used")),
                "use it, or remove it",
            )
            .about(name)
        })
        .collect()
}

// Step 8: the result.

struct Parts<'a, 'b> {
    builder: &'a LanguageBuilder,
    lexer: Lexer,
    nodes: Vec<Node>,
    rules: Vec<Rule>,
    exprs: Vec<Expr>,
    externs: Vec<Extern>,
    root: Callee,
    vocabulary: Vocabulary<'b>,
    lowering: Lowering<'b>,
    lowered_kinds: HashMap<String, SyntaxKind>,
    schema: super::Schema,
    warnings: Vec<Diagnostic>,
}

fn assemble(parts: Parts<'_, '_>) -> Language {
    let Parts {
        builder,
        lexer,
        nodes,
        rules,
        exprs,
        externs,
        root,
        vocabulary,
        lowering,
        lowered_kinds,
        schema,
        warnings,
    } = parts;
    let closers = lexer_closers(&lowering);
    let Vocabulary {
        kinds,
        tokens,
        literals,
        glued,
        contextual,
        lines,
        ..
    } = vocabulary;
    Language {
        name: builder.name.clone(),
        lexer,
        nodes,
        rules,
        exprs,
        externs,
        root,
        kind_names: kinds.names,
        node_kinds: lowered_kinds,
        declared: builder
            .tokens
            .iter()
            .filter(|token| {
                !token.trivia
                    && !matches!(token.shape, TokenShape::Part | TokenShape::Strings { .. })
            })
            .map(|token| token.name.clone())
            .collect(),
        custom_tokens: builder
            .tokens
            .iter()
            .filter(|token| matches!(token.shape, TokenShape::External(_)))
            .map(|token| token.name.clone())
            .collect(),
        token_kinds: tokens
            .into_iter()
            .map(|(name, kind)| (name.to_owned(), kind))
            .chain(literals)
            .chain(glued.into_iter().map(|(text, symbol)| (text, symbol.kind)))
            .collect(),
        callees: lowering
            .callees
            .iter()
            .map(|(&name, &callee)| (name.to_owned(), callee))
            .collect(),
        newline: lines.map(|(newline, _)| newline),
        terminators: lowering.extra_terminators,
        closers,
        words: contextual
            .iter()
            .map(|(&kind, word)| (word.clone(), kind))
            .collect(),
        parse_limits: builder.parse_limits,
        schema,
        warnings,
    }
}

/// The kinds that close a block, before which a terminator may be left out.
fn lexer_closers(lowering: &Lowering<'_>) -> KindSet {
    let mut closers = KindSet::default();
    for &(_, close) in &lowering.blocks {
        closers.insert(close);
    }
    closers
}
