//! Checks a grammar and works out every decision it takes.
//!
//! A decision looks at up to [`LOOKAHEAD`] tokens. For each alternative, the analysis
//! computes the token strings that can come next if it is taken, including what
//! follows the rule. Alternatives whose strings overlap cannot be told apart, and the
//! grammar is refused.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::SyntaxKind;

use crate::Diagnostic;

use super::ir::{
    Callee, ENTER, Entry, Expr, Extern, KindSet, Node, NodeId, Operation, PASS, Rule, Start,
    Symbol, Table,
};
use super::problem::{error, technically, to_fix};

/// The most tokens a decision looks at.
const LOOKAHEAD: usize = 3;

/// A token in a lookahead string.
type Token = u32;
/// Stands for a token that code of the language's own will read.
const ANY: Token = u32::MAX;
/// The end of the file.
const END: Token = u32::MAX - 1;

/// Up to [`LOOKAHEAD`] tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Text {
    len: u8,
    tokens: [Token; LOOKAHEAD],
}

type Set = BTreeSet<Text>;

/// The most strings one set of lookahead strings may hold, and all of them together.
/// With three tokens their number grows with the cube of the number of tokens, so a
/// large grammar could take all memory.
const MAX_SET: usize = 1 << 17;
const MAX_ALL_SETS: usize = 1 << 22;

const EMPTY: Text = Text {
    len: 0,
    tokens: [0; LOOKAHEAD],
};

impl Text {
    fn push(&mut self, token: Token, k: usize) {
        let len = usize::from(self.len);
        // Nothing comes after the end of the file.
        if len < k && (len == 0 || self.tokens[len - 1] != END) {
            self.tokens[len] = token;
            self.len += 1;
        }
    }

    fn is_full(self, k: usize) -> bool {
        let len = usize::from(self.len);
        len == k || (len > 0 && self.tokens[len - 1] == END)
    }

    fn first(self) -> Option<Token> {
        (self.len > 0).then(|| self.tokens[0])
    }
}

fn nothing_more() -> Set {
    Set::from([EMPTY])
}

/// Every string of `a` followed by every string of `b`, cut to `k` tokens.
fn join(a: &Set, b: &Set, k: usize) -> Set {
    let mut joined = Set::new();
    // The strings of `b` cut to the room each length of `a` leaves. After one or two
    // tokens, few of them differ.
    let mut cut: [Option<Set>; LOOKAHEAD] = Default::default();
    for &start in a {
        if start.is_full(k) {
            joined.insert(start);
            continue;
        }
        // The result is thrown away once it is too large.
        if joined.len() > MAX_SET {
            break;
        }
        // A string that is not full is shorter than `k`.
        let length = usize::from(start.len);
        let ends = cut[length].get_or_insert_with(|| {
            let room = (k - length) as u8;
            b.iter()
                .map(|&end| {
                    let mut short = EMPTY;
                    short.len = end.len.min(room);
                    let kept = usize::from(short.len);
                    short.tokens[..kept].copy_from_slice(&end.tokens[..kept]);
                    short
                })
                .collect()
        });
        for &end in ends.iter() {
            let mut text = start;
            for &token in &end.tokens[..usize::from(end.len)] {
                text.push(token, k);
            }
            joined.insert(text);
        }
    }
    joined
}

fn token_of(kind: SyntaxKind) -> Token {
    if kind == SyntaxKind::EOF {
        END
    } else {
        u32::from(kind.0)
    }
}

/// A grammar as the analysis sees it.
pub(super) struct Input<'a> {
    pub(super) nodes: &'a [Node],
    pub(super) rules: &'a [Rule],
    pub(super) exprs: &'a [Expr],
    pub(super) externs: &'a [Extern],
    pub(super) root: Callee,
    /// The kind of a line ending that terminates, if lines do.
    pub(super) newline: Option<SyntaxKind>,
    /// Literals that terminate as a line ending does.
    pub(super) terminators: &'a [Symbol],
    /// The closing kinds of blocks, before which a terminator may be left out.
    pub(super) closers: &'a [SyntaxKind],
    pub(super) contextual: &'a HashMap<SyntaxKind, String>,
    pub(super) identifiers: &'a [SyntaxKind],
    /// The name of each kind.
    pub(super) names: &'a [String],
}

impl Input<'_> {
    fn words(&self) -> Words<'_> {
        Words {
            contextual: self.contextual,
            identifiers: self.identifiers,
        }
    }
}

#[derive(Default)]
pub(super) struct Output {
    pub(super) tables: HashMap<NodeId, Table>,
    pub(super) expr_tables: Vec<Table>,
    /// Which parts can match nothing.
    pub(super) nullable: Vec<bool>,
    /// For each repetition and list: what can follow it.
    pub(super) after: HashMap<NodeId, KindSet>,
    /// For each repetition and list: what its body or item can start with.
    pub(super) starts: HashMap<NodeId, KindSet>,
    pub(super) errors: Vec<Diagnostic>,
    /// For each list that a required token follows: what can follow that token.
    pub(super) beyond: HashMap<NodeId, KindSet>,
    /// What can follow each rule and expression, as the kinds that can come first.
    pub(super) follow_rule: Vec<KindSet>,
    pub(super) follow_expr: Vec<KindSet>,
}

/// Why two choices cannot be told apart.
struct Clash {
    choices: (u16, u16),
    tokens: Vec<Token>,
}

/// What tells words apart from other tokens.
struct Words<'a> {
    contextual: &'a HashMap<SyntaxKind, String>,
    /// Kinds of identifier, which a contextual word also is.
    identifiers: &'a [SyntaxKind],
}

impl Words<'_> {
    fn is_contextual(&self, token: Token) -> bool {
        u16::try_from(token).is_ok_and(|kind| self.contextual.contains_key(&SyntaxKind(kind)))
    }
}

fn build_table(
    strings: &[(u16, Text)],
    depth: usize,
    k: usize,
    path: &mut Vec<Token>,
    words: &Words<'_>,
) -> Result<Table, Clash> {
    let mut by_token: BTreeMap<Token, Vec<(u16, Text)>> = BTreeMap::new();
    let mut anything = Vec::new();
    for &(choice, text) in strings {
        match text
            .tokens
            .get(depth)
            .filter(|_| depth < usize::from(text.len))
        {
            Some(&token) if token != ANY => {
                by_token.entry(token).or_default().push((choice, text));
            }
            _ => anything.push((choice, text)),
        }
    }

    let clash = |members: &[(u16, Text)], path: &[Token]| {
        let first = members[0].0;
        let other = members.iter().map(|m| m.0).find(|&c| c != first)?;
        Some(Clash {
            choices: (first.min(other), first.max(other)),
            tokens: path.to_vec(),
        })
    };

    // A contextual word is also an identifier, so where the word is looked for, the
    // ways that take any identifier are open too.
    let identifier_ways: Vec<(u16, Text)> = words
        .identifiers
        .iter()
        .filter_map(|&kind| by_token.get(&token_of(kind)))
        .flatten()
        .copied()
        .collect();

    let mut table = Table::default();
    for (token, mut members) in by_token {
        members.extend(anything.iter().copied());
        let mut entry = |members: &[(u16, Text)]| -> Result<Entry, Clash> {
            path.push(token);
            let entry = match clash(members, path) {
                None => Ok(Entry::Take(members[0].0)),
                Some(_) if depth + 1 < k && token != END => {
                    build_table(members, depth + 1, k, path, words)
                        .map(|further| Entry::Further(Box::new(further)))
                }
                Some(found) => Err(found),
            };
            path.pop();
            entry
        };
        let entry = if words.is_contextual(token) && !identifier_ways.is_empty() {
            let mut open = members.clone();
            open.extend(identifier_ways.iter().copied());
            // If both ways can go on alike, the word means what it was declared to.
            entry(&open).or_else(|_| entry(&members))?
        } else {
            entry(&members)?
        };

        if token == END {
            table.end = entry;
            continue;
        }
        // Token numbers other than the special ones came from a `u16`.
        let kind = SyntaxKind(token as u16);
        match words.contextual.get(&kind) {
            Some(word) => table.words.push((word.clone(), entry)),
            None => {
                let index = usize::from(kind.0);
                if table.kinds.len() <= index {
                    table.kinds.resize_with(index + 1, Entry::default);
                }
                table.kinds[index] = entry;
            }
        }
    }

    if !anything.is_empty() {
        if let Some(found) = clash(&anything, path) {
            return Err(found);
        }
        table.otherwise = Some(anything[0].0);
    } else if depth > 0 {
        // The first tokens already narrow it down; let one alternative report what
        // is wrong with the rest.
        table.otherwise = strings.iter().map(|s| s.0).min();
    }
    Ok(table)
}

/// What one pass knows about the whole grammar, for strings of `k` tokens.
struct Sets<'a> {
    input: &'a Input<'a>,
    k: usize,
    first_rule: Vec<Set>,
    first_expr: Vec<Set>,
    follow_rule: Vec<Set>,
    follow_expr: Vec<Set>,
    /// For each list that a required token follows: what can follow that token.
    beyond: HashMap<NodeId, Set>,
}

impl Sets<'_> {
    /// Whether the sets grew past what is worth computing. They are then incomplete.
    fn too_large(&self) -> bool {
        let sets = [
            &self.first_rule,
            &self.first_expr,
            &self.follow_rule,
            &self.follow_expr,
        ];
        let sizes = sets.into_iter().flatten().map(Set::len);
        sizes.clone().any(|size| size > MAX_SET) || sizes.sum::<usize>() > MAX_ALL_SETS
    }
}

impl<'a> Sets<'a> {
    fn symbol(&self, symbol: &Symbol) -> Set {
        let mut text = EMPTY;
        for &part in &symbol.parts {
            text.push(token_of(part), self.k);
        }
        Set::from([text])
    }

    fn token(&self, kind: SyntaxKind, tail: &Set) -> Set {
        let mut text = EMPTY;
        text.push(token_of(kind), self.k);
        join(&Set::from([text]), tail, self.k)
    }

    /// What can come next at `node` when `tail` can come after it.
    fn first(&self, node: NodeId, tail: &Set) -> Set {
        match &self.input.nodes[node] {
            Node::Token(kind) | Node::Contextual(kind) => self.token(*kind, tail),
            Node::Glued { parts, kind } => {
                let symbol = Symbol {
                    parts: parts.clone(),
                    kind: *kind,
                };
                join(&self.symbol(&symbol), tail, self.k)
            }
            Node::Rule(rule) => join(&self.first_rule[*rule], tail, self.k),
            Node::Expr(expr) => join(&self.first_expr[*expr], tail, self.k),
            Node::Extern(index) => join(&self.first_extern(*index), tail, self.k),
            Node::Sequence(parts) => parts
                .iter()
                .rev()
                .fold(tail.clone(), |tail, &part| self.first(part, &tail)),
            Node::Choice { alternatives, .. } => alternatives
                .iter()
                .flat_map(|&alternative| self.first(alternative, tail))
                .collect(),
            Node::Optional { body, .. } => {
                let mut set = self.first(*body, tail);
                set.extend(tail.iter().copied());
                set
            }
            Node::Many { body, .. } => self.grow(tail.clone(), |so_far| {
                let mut set = self.first(*body, so_far);
                set.extend(tail.iter().copied());
                set
            }),
            Node::Separated {
                item,
                separator,
                strict,
                ..
            } => {
                let after = self.after_item(*item, separator, *strict, tail);
                let mut set = self.first(*item, &after);
                set.extend(tail.iter().copied());
                set
            }
            Node::Terminated(body) => {
                let end = self.terminator(tail);
                self.first(*body, &end)
            }
            Node::Look(body) => {
                let wanted = self.starts(*body);
                tail.iter()
                    .filter(|text| text.first().is_none_or(|t| t == ANY || wanted.contains(&t)))
                    .copied()
                    .collect()
            }
            Node::Not(body) => match self.input.nodes[*body] {
                // Only a single token can be ruled out by looking at one.
                Node::Token(kind) => tail
                    .iter()
                    .filter(|text| text.first() != Some(token_of(kind)))
                    .copied()
                    .collect(),
                _ => tail.clone(),
            },
            Node::Attempt(body) | Node::Block(body) => self.first(*body, tail),
            Node::Missing => tail.clone(),
        }
    }

    /// The first tokens of a part.
    fn starts(&self, node: NodeId) -> BTreeSet<Token> {
        self.first(node, &nothing_more())
            .into_iter()
            .filter_map(Text::first)
            .collect()
    }

    fn first_extern(&self, index: usize) -> Set {
        let unknown = |mut text: Text| {
            while usize::from(text.len) < self.k {
                text.push(ANY, self.k);
            }
            text
        };
        let first = &self.input.externs[index].first;
        if first.is_empty() {
            return Set::from([unknown(EMPTY)]);
        }
        first
            .iter()
            .map(|&kind| {
                let mut text = EMPTY;
                text.push(token_of(kind), self.k);
                unknown(text)
            })
            .collect()
    }

    /// Applies `step` until the set stops changing.
    fn grow(&self, start: Set, step: impl Fn(&Set) -> Set) -> Set {
        let mut set = start;
        loop {
            let next = step(&set);
            if next == set {
                return set;
            }
            set = next;
        }
    }

    /// What can follow an item of a separated list.
    fn after_item(&self, item: NodeId, separator: &Symbol, strict: bool, tail: &Set) -> Set {
        self.grow(tail.clone(), |after| {
            let mut set = tail.clone();
            let more = self.first(item, after);
            let separator = self.symbol(separator);
            set.extend(join(&separator, &more, self.k));
            if !strict {
                set.extend(join(&separator, tail, self.k));
            }
            set
        })
    }

    /// What can come at the end of a terminated part.
    fn terminator(&self, tail: &Set) -> Set {
        let mut set: Set = self
            .input
            .newline
            .iter()
            .flat_map(|&kind| self.token(kind, tail))
            .collect();
        for symbol in self.input.terminators {
            set.extend(join(&self.symbol(symbol), tail, self.k));
        }
        let closers: Vec<Token> = self
            .input
            .closers
            .iter()
            .map(|&kind| token_of(kind))
            .collect();
        set.extend(
            tail.iter()
                .filter(|text| {
                    text.first()
                        .is_none_or(|t| t == END || t == ANY || closers.contains(&t))
                })
                .copied(),
        );
        set
    }

    /// What can follow an operand of an expression: its operators, then what follows
    /// the expression.
    fn after_operand(&self, expr: usize) -> Set {
        let follow = &self.follow_expr[expr];
        self.grow(follow.clone(), |after| {
            let mut set = follow.clone();
            for continuation in &self.input.exprs[expr].continuations {
                match continuation.operation {
                    Operation::Infix { .. } => {
                        let operand = join(&self.first_expr[expr], after, self.k);
                        set.extend(join(&self.symbol(&continuation.symbol), &operand, self.k));
                    }
                    Operation::Postfix { body } => set.extend(self.first(body, after)),
                }
            }
            set
        })
    }

    fn new(input: &'a Input<'a>, k: usize) -> Self {
        let mut sets = Self {
            input,
            k,
            first_rule: vec![Set::new(); input.rules.len()],
            first_expr: vec![Set::new(); input.exprs.len()],
            follow_rule: vec![Set::new(); input.rules.len()],
            follow_expr: vec![Set::new(); input.exprs.len()],
            beyond: HashMap::new(),
        };
        sets.compute_first();
        sets
    }

    fn compute_first(&mut self) {
        loop {
            let mut changed = false;
            for (index, rule) in self.input.rules.iter().enumerate() {
                let set = self.first(rule.body, &nothing_more());
                if set != self.first_rule[index] {
                    self.first_rule[index] = set;
                    changed = true;
                    if self.too_large() {
                        return;
                    }
                }
            }
            for (index, expr) in self.input.exprs.iter().enumerate() {
                let mut set = Set::new();
                for start in &expr.starts {
                    match start {
                        Start::Atom { body, .. } => {
                            set.extend(self.first(*body, &nothing_more()));
                        }
                        Start::Prefix { symbol, .. } => {
                            set.extend(join(&self.symbol(symbol), &self.first_expr[index], self.k))
                        }
                    }
                }
                let mut more = Set::new();
                for continuation in &expr.continuations {
                    match continuation.operation {
                        Operation::Infix { .. } => more.extend(join(
                            &self.symbol(&continuation.symbol),
                            &self.first_expr[index],
                            self.k,
                        )),
                        Operation::Postfix { body } => {
                            more.extend(self.first(body, &nothing_more()));
                        }
                    }
                }
                set.extend(join(&self.first_expr[index], &more, self.k));
                if set != self.first_expr[index] {
                    self.first_expr[index] = set;
                    changed = true;
                }
            }
            if !changed || self.too_large() {
                return;
            }
        }
    }

    /// Records what follows every rule and expression used inside `node`.
    fn follow(&mut self, node: NodeId, tail: &Set) -> bool {
        let input = self.input;
        match &input.nodes[node] {
            Node::Rule(rule) => extend(&mut self.follow_rule[*rule], tail),
            Node::Expr(expr) => extend(&mut self.follow_expr[*expr], tail),
            Node::Sequence(parts) => {
                let mut changed = false;
                let mut after = tail.clone();
                // What follows the part to the right of the one being visited.
                let mut beyond: Option<Set> = None;
                for (index, &part) in parts.iter().enumerate().rev() {
                    let closed = parts.get(index + 1).is_some_and(|&next| {
                        matches!(
                            input.nodes[next],
                            Node::Token(_) | Node::Contextual(_) | Node::Glued { .. }
                        )
                    });
                    if let (true, Some(beyond)) = (closed, &beyond)
                        && matches!(input.nodes[part], Node::Separated { .. })
                    {
                        self.beyond
                            .entry(part)
                            .or_default()
                            .extend(beyond.iter().copied());
                    }
                    changed |= self.follow(part, &after);
                    beyond = Some(after.clone());
                    after = self.first(part, &after);
                }
                changed
            }
            Node::Choice { alternatives, .. } => alternatives
                .iter()
                .fold(false, |changed, &part| self.follow(part, tail) | changed),
            Node::Optional { body, .. } | Node::Attempt(body) | Node::Block(body) => {
                self.follow(*body, tail)
            }
            Node::Many { body, .. } => {
                let after = self.first(node, tail);
                self.follow(*body, &after)
            }
            Node::Separated {
                item,
                separator,
                strict,
                ..
            } => {
                let after = self.after_item(*item, separator, *strict, tail);
                self.follow(*item, &after)
            }
            Node::Terminated(body) => {
                let end = self.terminator(tail);
                self.follow(*body, &end)
            }
            Node::Token(_)
            | Node::Contextual(_)
            | Node::Glued { .. }
            | Node::Extern(_)
            | Node::Look(_)
            | Node::Not(_)
            | Node::Missing => false,
        }
    }

    /// Computes what follows everything. With `operators` off, the operand of an
    /// expression is followed only by what its own rule puts after it.
    fn compute_follow(&mut self, operators: bool) {
        if let Callee::Rule(root) = self.input.root {
            self.follow_rule[root].insert({
                let mut end = EMPTY;
                end.push(END, self.k);
                end
            });
        }
        loop {
            let mut changed = false;
            let input = self.input;
            for (index, rule) in input.rules.iter().enumerate() {
                let tail = self.follow_rule[index].clone();
                changed |= self.follow(rule.body, &tail);
                if self.too_large() {
                    return;
                }
            }
            for (index, expr) in input.exprs.iter().enumerate() {
                let tail = if operators {
                    self.after_operand(index)
                } else {
                    Set::new()
                };
                for body in bodies(expr) {
                    changed |= self.follow(body, &tail);
                }
            }
            if !changed || self.too_large() {
                return;
            }
        }
    }
}

fn extend(set: &mut Set, more: &Set) -> bool {
    let before = set.len();
    set.extend(more.iter().copied());
    set.len() != before
}

/// The parts of an expression that are written as patterns.
pub(super) fn bodies(expr: &Expr) -> impl Iterator<Item = NodeId> {
    let atoms = expr.starts.iter().filter_map(|start| match start {
        Start::Atom { body, .. } => Some(*body),
        Start::Prefix { .. } => None,
    });
    let postfix =
        expr.continuations
            .iter()
            .filter_map(|continuation| match continuation.operation {
                Operation::Postfix { body } => Some(body),
                Operation::Infix { .. } => None,
            });
    atoms.chain(postfix)
}

/// Which parts can match nothing.
fn nullable(input: &Input<'_>) -> Vec<bool> {
    let mut rules = vec![false; input.rules.len()];
    let mut nodes = vec![false; input.nodes.len()];
    loop {
        let mut changed = false;
        // Parts are numbered after the parts they contain.
        for (index, node) in input.nodes.iter().enumerate() {
            let value = match node {
                Node::Token(_)
                | Node::Contextual(_)
                | Node::Glued { .. }
                | Node::Expr(_)
                | Node::Extern(_)
                | Node::Missing => false,
                Node::Rule(rule) => rules[*rule],
                Node::Sequence(parts) => parts.iter().all(|&part| nodes[part]),
                Node::Choice { alternatives, .. } => alternatives.iter().any(|&part| nodes[part]),
                Node::Optional { .. }
                | Node::Many { .. }
                | Node::Separated { .. }
                | Node::Look(_)
                | Node::Not(_)
                // An attempt that fails is passed over.
                | Node::Attempt(_) => true,
                Node::Terminated(body) | Node::Block(body) => nodes[*body],
            };
            changed |= value && !nodes[index];
            nodes[index] |= value;
        }
        for (index, rule) in input.rules.iter().enumerate() {
            changed |= nodes[rule.body] && !rules[index];
            rules[index] |= nodes[rule.body];
        }
        if !changed {
            return nodes;
        }
    }
}

/// What can be parsed first inside a part, before any token is taken.
fn left(input: &Input<'_>, nullable: &[bool], node: NodeId, found: &mut Vec<Callee>) {
    match &input.nodes[node] {
        Node::Rule(rule) => found.push(Callee::Rule(*rule)),
        Node::Expr(expr) => found.push(Callee::Expr(*expr)),
        Node::Extern(index) => found.push(Callee::Extern(*index)),
        Node::Sequence(parts) => {
            for &part in parts {
                left(input, nullable, part, found);
                if !nullable[part] {
                    break;
                }
            }
        }
        Node::Choice { alternatives, .. } => {
            for &part in alternatives {
                left(input, nullable, part, found);
            }
        }
        Node::Optional { body, .. }
        | Node::Many { body, .. }
        | Node::Terminated(body)
        | Node::Block(body)
        | Node::Look(body)
        | Node::Not(body)
        | Node::Attempt(body) => left(input, nullable, *body, found),
        Node::Separated { item, .. } => left(input, nullable, *item, found),
        Node::Token(_) | Node::Contextual(_) | Node::Glued { .. } | Node::Missing => {}
    }
}

fn left_of(input: &Input<'_>, nullable: &[bool], callee: Callee) -> Vec<Callee> {
    let mut found = Vec::new();
    match callee {
        Callee::Rule(rule) => left(input, nullable, input.rules[rule].body, &mut found),
        Callee::Expr(expr) => {
            for start in &input.exprs[expr].starts {
                if let Start::Atom { body, .. } = start {
                    left(input, nullable, *body, &mut found);
                }
            }
        }
        Callee::Extern(_) => {}
    }
    found
}

/// Everything reachable from `starts` without taking a token, `starts` included.
fn left_closure(input: &Input<'_>, nullable: &[bool], starts: &[Callee]) -> BTreeSet<Callee> {
    let mut found = BTreeSet::new();
    let mut pending = starts.to_vec();
    while let Some(callee) = pending.pop() {
        if found.insert(callee) {
            pending.extend(left_of(input, nullable, callee));
        }
    }
    found
}

fn callee_name<'a>(input: &Input<'a>, callee: Callee) -> &'a str {
    match callee {
        Callee::Rule(rule) => &input.rules[rule].name,
        Callee::Expr(expr) => &input.exprs[expr].name,
        Callee::Extern(index) => &input.externs[index].name,
    }
}

/// A path from `start` back to itself without taking a token.
fn cycle(input: &Input<'_>, nullable: &[bool], start: Callee) -> Option<Vec<Callee>> {
    let mut paths = vec![vec![start]];
    let mut seen = BTreeSet::new();
    // Shortest paths first.
    while !paths.is_empty() {
        let mut longer = Vec::new();
        for path in paths {
            let last = *path.last()?;
            for next in left_of(input, nullable, last) {
                let mut path = path.clone();
                path.push(next);
                if next == start {
                    return Some(path);
                }
                if seen.insert(next) {
                    longer.push(path);
                }
            }
        }
        paths = longer;
    }
    None
}

fn left_recursion(input: &Input<'_>, nullable: &[bool]) -> Vec<Diagnostic> {
    let callees = (0..input.rules.len())
        .map(Callee::Rule)
        .chain((0..input.exprs.len()).map(Callee::Expr));
    let mut reported = BTreeSet::new();
    let mut errors = Vec::new();
    for callee in callees {
        if reported.contains(&callee) {
            continue;
        }
        let Some(path) = cycle(input, nullable, callee) else {
            continue;
        };
        let names: Vec<_> = path.iter().map(|&c| callee_name(input, c)).collect();
        let name = callee_name(input, callee);
        let message = format!(
            "`{name}` can start with itself ({}), so parsing it would never take a token",
            names.join(" -> ")
        );
        let problem = technically(
            error("LM-G002", message),
            "this is left recursion, which a parser that reads from left to right cannot handle",
        );
        errors.push(to_fix(problem, "move operators into an `expr` block"));
        reported.extend(path);
    }
    errors
}

/// The name of a kind, for messages.
fn name_of<'a>(input: &Input<'a>, kind: SyntaxKind) -> &'a str {
    input
        .names
        .get(usize::from(kind.0))
        .map_or("?", String::as_str)
}

/// A short name for a part, for messages.
fn show(input: &Input<'_>, node: NodeId) -> String {
    let kind_name = |kind: SyntaxKind| format!("`{}`", name_of(input, kind));
    match &input.nodes[node] {
        Node::Token(kind) | Node::Contextual(kind) | Node::Glued { kind, .. } => kind_name(*kind),
        Node::Rule(rule) => format!("`{}`", input.rules[*rule].name),
        Node::Expr(expr) => format!("`{}`", input.exprs[*expr].name),
        Node::Extern(index) => format!("`{}`", input.externs[*index].name),
        Node::Sequence(parts) => parts
            .first()
            .map_or_else(|| "part".to_owned(), |&part| show(input, part)),
        Node::Choice { alternatives, .. } => alternatives
            .iter()
            .map(|&part| show(input, part))
            .collect::<Vec<_>>()
            .join(" or "),
        Node::Optional { body, .. }
        | Node::Many { body, .. }
        | Node::Terminated(body)
        | Node::Block(body)
        | Node::Look(body)
        | Node::Not(body)
        | Node::Attempt(body) => show(input, *body),
        Node::Separated { item, .. } => show(input, *item),
        Node::Missing => "part".to_owned(),
    }
}

fn too_costly(holder: &str, k: usize) -> Diagnostic {
    let message = format!(
        "in `{holder}`, a choice needs to look {k} tokens ahead, which is more than can be \
         checked for a grammar this large"
    );
    to_fix(
        error("LM-G016", message),
        "factor out the common start, or use `try` / `extern rule`",
    )
}

fn show_token(input: &Input<'_>, token: Token) -> String {
    match token {
        END => "the end of the file".to_owned(),
        ANY => "any token".to_owned(),
        // Other token numbers came from a `u16`.
        token => format!("`{}`", name_of(input, SyntaxKind(token as u16))),
    }
}

/// One decision of the grammar and where it is.
struct Decision<'a> {
    node: NodeId,
    holder: &'a str,
    tail: Set,
    /// What the rest of the same rule can start with.
    rest: Vec<Callee>,
}

/// Finds every decision under `node`.
fn decisions<'a>(
    sets: &Sets<'a>,
    nullable: &[bool],
    holder: &'a str,
    node: NodeId,
    tail: &Set,
    rest: &[Callee],
    found: &mut Vec<Decision<'a>>,
) {
    let input = sets.input;
    let visit = |node: NodeId, tail: &Set, rest: &[Callee], found: &mut Vec<_>| {
        decisions(sets, nullable, holder, node, tail, rest, found);
    };
    let here = || Decision {
        node,
        holder,
        tail: tail.clone(),
        rest: rest.to_vec(),
    };
    match &input.nodes[node] {
        Node::Sequence(parts) => {
            let mut after = tail.clone();
            let mut following = rest.to_vec();
            for &part in parts.iter().rev() {
                visit(part, &after, &following, found);
                after = sets.first(part, &after);
                let mut starts = Vec::new();
                left(input, nullable, part, &mut starts);
                if nullable[part] {
                    starts.append(&mut following);
                }
                following = starts;
            }
        }
        Node::Choice { alternatives, .. } => {
            found.push(here());
            for &part in alternatives {
                visit(part, tail, rest, found);
            }
        }
        Node::Optional { body, .. } => {
            found.push(here());
            visit(*body, tail, rest, found);
        }
        Node::Many { body, .. } => {
            found.push(here());
            let after = sets.first(node, tail);
            visit(*body, &after, rest, found);
        }
        Node::Separated {
            item,
            separator,
            strict,
            ..
        } => {
            found.push(here());
            let after = sets.after_item(*item, separator, *strict, tail);
            visit(*item, &after, rest, found);
        }
        Node::Terminated(body) => {
            let end = sets.terminator(tail);
            visit(*body, &end, rest, found);
        }
        Node::Attempt(body) | Node::Block(body) => visit(*body, tail, rest, found),
        Node::Token(_)
        | Node::Contextual(_)
        | Node::Glued { .. }
        | Node::Rule(_)
        | Node::Expr(_)
        | Node::Extern(_)
        | Node::Look(_)
        | Node::Not(_)
        | Node::Missing => {}
    }
}

fn labelled(choice: u16, set: Set) -> impl Iterator<Item = (u16, Text)> {
    set.into_iter().map(move |text| (choice, text))
}

fn resolve(
    sets: &Sets<'_>,
    nullable: &[bool],
    decision: &Decision<'_>,
) -> Result<Table, Box<Diagnostic>> {
    let input = sets.input;
    let k = sets.k;
    let node = decision.node;
    let tail = &decision.tail;
    let table =
        |strings: Vec<(u16, Text)>| build_table(&strings, 0, k, &mut Vec::new(), &input.words());

    // The common start of a part and of what follows it, if they have one.
    let common = |body: NodeId| {
        let mut starts = Vec::new();
        left(input, nullable, body, &mut starts);
        let inside = left_closure(input, nullable, &starts);
        let after = left_closure(input, nullable, &decision.rest);
        decision
            .rest
            .iter()
            .chain(&after)
            .find(|callee| inside.contains(callee))
            .map(|&callee| callee_name(input, callee))
    };
    let holder = decision.holder;
    let overlap = |body: NodeId, role: Role, clash: &Clash| {
        collision(input, holder, body, role, clash, common(body))
    };

    match &input.nodes[node] {
        Node::Choice {
            alternatives,
            attempts,
            ..
        } => {
            let strings = alternatives
                .iter()
                .zip(0..)
                .filter(|(part, _)| !attempts.contains(part))
                .flat_map(|(&part, index)| labelled(index, sets.first(part, tail)))
                .collect();
            table(strings).map_err(|clash| {
                let name = |choice: u16| show(input, alternatives[usize::from(choice)]);
                Box::new(choice_error(
                    input,
                    holder,
                    name(clash.choices.0),
                    name(clash.choices.1),
                    &clash,
                ))
            })
        }
        Node::Optional { body, .. } => {
            let mut strings: Vec<_> = labelled(ENTER, sets.first(*body, tail)).collect();
            strings.extend(labelled(PASS, tail.clone()));
            table(strings).map_err(|clash| Box::new(overlap(*body, Role::Optional, &clash)))
        }
        Node::Many { body, .. } => {
            let after = sets.first(node, tail);
            let mut strings: Vec<_> = labelled(ENTER, sets.first(*body, &after)).collect();
            strings.extend(labelled(PASS, tail.clone()));
            table(strings).map_err(|clash| Box::new(overlap(*body, Role::Repeated, &clash)))
        }
        Node::Separated {
            item,
            separator,
            strict,
            ..
        } => {
            let after = sets.after_item(*item, separator, *strict, tail);
            let list_error = |clash: Clash| Box::new(overlap(*item, Role::List, &clash));
            // A separator must not be confused with what follows the list.
            let more = sets.first(*item, &after);
            let mut separators: Vec<_> =
                labelled(ENTER, join(&sets.symbol(separator), &more, sets.k)).collect();
            separators.extend(labelled(PASS, tail.clone()));
            table(separators).map_err(list_error)?;
            let mut strings: Vec<_> = labelled(ENTER, more).collect();
            strings.extend(labelled(PASS, tail.clone()));
            table(strings).map_err(list_error)
        }
        _ => Ok(Table::default()),
    }
}

/// How a part is optional or repeated.
#[derive(Clone, Copy)]
enum Role {
    Optional,
    Repeated,
    List,
}

impl Role {
    /// What the part is called in a message.
    fn noun(self) -> &'static str {
        match self {
            Self::Optional => "optional",
            Self::Repeated => "repeated",
            Self::List => "list of",
        }
    }

    /// What the parser cannot tell.
    fn cannot_tell(self) -> &'static str {
        match self {
            Self::Optional => "whether the optional part is there",
            Self::Repeated => "where the repetition ends",
            Self::List => "where the list ends",
        }
    }

    /// What a token could do besides following the part.
    fn continuing(self) -> &'static str {
        match self {
            Self::Optional => "start the optional part",
            Self::Repeated => "continue the repetition",
            Self::List => "continue the list",
        }
    }
}

/// A part and what follows it that cannot be told apart. If they begin with the same
/// rule it is a clash of choices (LM-G003); otherwise a token that both could take
/// (LM-G004).
fn collision(
    input: &Input<'_>,
    holder: &str,
    body: NodeId,
    role: Role,
    clash: &Clash,
    common: Option<&str>,
) -> Diagnostic {
    let part = format!("{} {}", role.noun(), show(input, body));
    match common {
        Some(start) => {
            let message = format!(
                "in `{holder}`, the {part} and what follows it both start with `{start}`, so the \
                 parser cannot tell {}",
                role.cannot_tell()
            );
            let problem = technically(
                error("LM-G003", message),
                format!(
                    "the choice between going on and stopping is made by looking at most \
                     {LOOKAHEAD} tokens ahead (LL({LOOKAHEAD})), and both ways can begin with `{start}`"
                ),
            );
            to_fix(
                problem,
                "factor out the common start, for example by ending the repeated part with `;` \
                 or with a lookahead such as `&\"}\"`, or use `try` / `extern rule`",
            )
        }
        None => {
            let tokens: Vec<_> = clash.tokens.iter().map(|&t| show_token(input, t)).collect();
            let message = format!(
                "in `{holder}`, after the {part}, {} could either {} or belong to what follows \
                 it, and the parser cannot tell which",
                tokens.join(" "),
                role.continuing()
            );
            let problem = technically(
                error("LM-G004", message),
                format!(
                    "the first {LOOKAHEAD} tokens of the part and of what follows it overlap (their \
                     FIRST and FOLLOW sets collide)"
                ),
            );
            to_fix(
                problem,
                format!("make the {} part start differently from what follows it", {
                    match role {
                        Role::Optional => "optional",
                        Role::Repeated | Role::List => "repeated",
                    }
                }),
            )
        }
    }
}

fn choice_error(
    input: &Input<'_>,
    holder: &str,
    first: String,
    second: String,
    clash: &Clash,
) -> Diagnostic {
    let tokens: Vec<_> = clash.tokens.iter().map(|&t| show_token(input, t)).collect();
    let start = if tokens.is_empty() {
        "nothing".to_owned()
    } else {
        tokens.join(" ")
    };
    let message = format!(
        "in `{holder}`, {first} and {second} both start with {start}, and {LOOKAHEAD} tokens \
         are not enough to tell them apart"
    );
    let problem = technically(
        error("LM-G003", message),
        format!(
            "the parser decides by looking at most {LOOKAHEAD} tokens ahead (it is LL({LOOKAHEAD})), \
             and both choices have the same first {LOOKAHEAD} tokens"
        ),
    );
    to_fix(
        problem,
        "factor out the common start, or use `try` / `extern rule`",
    )
}

fn expr_table(sets: &Sets<'_>, index: usize) -> Result<Table, Box<Diagnostic>> {
    let input = sets.input;
    let expr = &input.exprs[index];
    let after = sets.after_operand(index);
    let strings: Vec<_> = expr
        .starts
        .iter()
        .zip(0..)
        .flat_map(|(start, choice)| {
            let set = match start {
                Start::Atom { body, .. } => sets.first(*body, &after),
                Start::Prefix { symbol, .. } => {
                    let operand = join(&sets.first_expr[index], &after, sets.k);
                    join(&sets.symbol(symbol), &operand, sets.k)
                }
            };
            labelled(choice, set)
        })
        .collect();
    build_table(&strings, 0, sets.k, &mut Vec::new(), &input.words()).map_err(|clash| {
        let name = |choice: u16| {
            let kind = match &expr.starts[usize::from(choice)] {
                Start::Atom { node, .. } | Start::Prefix { node, .. } => *node,
            };
            format!("`{}`", name_of(input, kind))
        };
        Box::new(choice_error(
            input,
            &expr.name,
            name(clash.choices.0),
            name(clash.choices.1),
            &clash,
        ))
    })
}

/// Operators that could mean two things.
fn operator_clashes(input: &Input<'_>) -> Vec<Diagnostic> {
    let mut sets = Sets::new(input, 1);
    sets.compute_follow(false);
    let mut errors = Vec::new();
    for (index, expr) in input.exprs.iter().enumerate() {
        let node_name = |kind: SyntaxKind| name_of(input, kind);
        for (position, continuation) in expr.continuations.iter().enumerate() {
            let symbol = name_of(input, continuation.symbol.kind);
            let twin = expr.continuations[..position]
                .iter()
                .find(|other| other.symbol.parts == continuation.symbol.parts);
            if let Some(twin) = twin {
                let (a, b) = (node_name(twin.node), node_name(continuation.node));
                let message = format!(
                    "in `{}`, `{symbol}` continues an expression as both `{a}` and `{b}`",
                    expr.name
                );
                errors.push(to_fix(
                    error("LM-G010", message),
                    "give the operators different symbols",
                ));
                continue;
            }
            let follows = continuation.symbol.parts.first().is_some_and(|&first| {
                sets.follow_expr[index]
                    .iter()
                    .any(|text| text.first() == Some(token_of(first)))
            });
            if follows {
                let name = node_name(continuation.node);
                let message = format!(
                    "in `{}`, after an expression, `{symbol}` could continue it as `{name}` or \
                     belong to what follows it",
                    expr.name
                );
                errors.push(to_fix(
                    error("LM-G010", message),
                    "use a different symbol for the operator, or for what follows",
                ));
            }
        }
    }
    errors
}

/// Repetitions whose body can match nothing.
fn empty_repetitions<'a>(
    input: &Input<'a>,
    nullable: &[bool],
    holders: &[(&'a str, NodeId)],
) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    for &(holder, root) in holders {
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            match &input.nodes[node] {
                Node::Many { body, .. } | Node::Separated { item: body, .. } if nullable[*body] => {
                    let message = format!(
                        "in `{holder}`, a repeated part can match nothing, so the repetition \
                         could go on without end"
                    );
                    errors.push(to_fix(
                        error("LM-G005", message),
                        "require at least one token inside the repetition",
                    ));
                }
                _ => {}
            }
            children(&input.nodes[node], &mut pending);
        }
    }
    errors
}

pub(super) fn children(node: &Node, into: &mut Vec<NodeId>) {
    match node {
        Node::Sequence(parts)
        | Node::Choice {
            alternatives: parts,
            ..
        } => into.extend(parts),
        Node::Optional { body, .. }
        | Node::Many { body, .. }
        | Node::Terminated(body)
        | Node::Block(body)
        | Node::Look(body)
        | Node::Not(body)
        | Node::Attempt(body) => into.push(*body),
        Node::Separated { item, .. } => into.push(*item),
        Node::Token(_)
        | Node::Contextual(_)
        | Node::Glued { .. }
        | Node::Rule(_)
        | Node::Expr(_)
        | Node::Extern(_)
        | Node::Missing => {}
    }
}

fn kinds(tokens: impl IntoIterator<Item = Token>) -> KindSet {
    let mut set = KindSet::default();
    for token in tokens {
        match token {
            ANY => {}
            END => set.insert(SyntaxKind::EOF),
            // Other token numbers came from a `u16`.
            token => set.insert(SyntaxKind(token as u16)),
        }
    }
    set
}

/// Checks the grammar. `unresolved` says that some names were not found, which makes
/// further questions about what can come next meaningless.
pub(super) fn analyze(input: &Input<'_>, unresolved: bool) -> Output {
    let nullable = nullable(input);
    let mut output = Output {
        expr_tables: vec![Table::default(); input.exprs.len()],
        nullable: nullable.clone(),
        ..Output::default()
    };
    let holders: Vec<(&str, NodeId)> = input
        .rules
        .iter()
        .map(|rule| (rule.name.as_str(), rule.body))
        .chain(
            input
                .exprs
                .iter()
                .flat_map(|expr| bodies(expr).map(|body| (expr.name.as_str(), body))),
        )
        .collect();

    let recursion = left_recursion(input, &nullable);
    let recursive = !recursion.is_empty();
    output.errors.extend(recursion);
    if recursive {
        return output;
    }
    let empty = empty_repetitions(input, &nullable, &holders);
    let broken = unresolved || !empty.is_empty();
    output.errors.extend(empty);
    if broken {
        return output;
    }

    // Most decisions need one token. Look further only for those that need more.
    let mut open_exprs: Vec<usize> = (0..input.exprs.len()).collect();
    let mut solved: BTreeSet<NodeId> = BTreeSet::new();
    // Decisions that need more tokens than tried so far.
    let mut pending: Vec<(String, NodeId)> = Vec::new();
    for k in 1..=LOOKAHEAD {
        let mut sets = Sets::new(input, k);
        if !sets.too_large() {
            sets.compute_follow(true);
        }
        if sets.too_large() {
            output
                .errors
                .extend(pending.drain(..).map(|(holder, _)| too_costly(&holder, k)));
            break;
        }
        let last = k == LOOKAHEAD;

        let mut found = Vec::new();
        for (index, rule) in input.rules.iter().enumerate() {
            let tail = sets.follow_rule[index].clone();
            decisions(
                &sets,
                &nullable,
                &rule.name,
                rule.body,
                &tail,
                &[],
                &mut found,
            );
        }
        for (index, expr) in input.exprs.iter().enumerate() {
            let tail = sets.after_operand(index);
            for body in bodies(expr) {
                decisions(&sets, &nullable, &expr.name, body, &tail, &[], &mut found);
            }
        }
        let mut unsolved = false;
        pending.clear();
        if k == 1 {
            let firsts = |sets: &[Set]| -> Vec<KindSet> {
                sets.iter()
                    .map(|set| kinds(set.iter().filter_map(|text| text.first())))
                    .collect()
            };
            output.follow_rule = firsts(&sets.follow_rule);
            output.follow_expr = firsts(&sets.follow_expr);
            for decision in &found {
                let body = match input.nodes[decision.node] {
                    Node::Many { body, .. } => body,
                    Node::Separated { item, .. } => item,
                    _ => continue,
                };
                let starts = sets.starts(body);
                output.after.insert(
                    decision.node,
                    kinds(decision.tail.iter().filter_map(|text| text.first())),
                );
                if let Some(beyond) = sets.beyond.get(&decision.node) {
                    output.beyond.insert(
                        decision.node,
                        kinds(beyond.iter().filter_map(|t| t.first())),
                    );
                }
                output.starts.insert(decision.node, kinds(starts));
            }
        }
        for decision in found {
            if solved.contains(&decision.node) {
                continue;
            }
            match resolve(&sets, &nullable, &decision) {
                Ok(table) => {
                    solved.insert(decision.node);
                    output.tables.insert(decision.node, table);
                }
                Err(error) if last => output.errors.push(*error),
                Err(_) => {
                    unsolved = true;
                    pending.push((decision.holder.to_owned(), decision.node));
                }
            }
        }
        open_exprs.retain(|&index| match expr_table(&sets, index) {
            Ok(table) => {
                output.expr_tables[index] = table;
                false
            }
            Err(error) => {
                if last {
                    output.errors.push(*error);
                }
                true
            }
        });
        if !unsolved && open_exprs.is_empty() {
            break;
        }
    }
    output.errors.extend(operator_clashes(input));
    output
}
