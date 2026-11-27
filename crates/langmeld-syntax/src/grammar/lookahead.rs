//! Where a grammar looks more than one token ahead.

use crate::SyntaxKind;

use super::Language;
use super::analyze::children;
use super::ir::{Entry, Node, NodeId, Operation, Start, Table};

/// A choice that the next token does not settle.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Lookahead {
    /// The rule or expression the choice is in.
    pub rule: String,
    /// The token that starts the ways that cannot yet be told apart.
    pub starts: String,
    /// How many tokens are looked at, counting that one.
    pub tokens: usize,
}

fn entry_depth(entry: &Entry) -> usize {
    match entry {
        Entry::Further(table) => 1 + table_depth(table),
        Entry::Miss | Entry::Take(_) => 1,
    }
}

fn table_depth(table: &Table) -> usize {
    table
        .kinds
        .iter()
        .chain(table.words.iter().map(|(_, entry)| entry))
        .chain([&table.end])
        .map(entry_depth)
        .max()
        .unwrap_or(1)
}

/// The tables of the decisions made in the parts reached from `roots`.
fn tables_in<'a>(language: &'a Language, roots: Vec<NodeId>, into: &mut Vec<&'a Table>) {
    let mut pending = roots;
    while let Some(node) = pending.pop() {
        into.extend(table_of(&language.nodes[node]));
        children(&language.nodes[node], &mut pending);
    }
}

/// Every table of decisions, with the rule that holds it.
fn tables(language: &Language) -> Vec<(&str, &Table)> {
    let mut result = Vec::new();
    for rule in &language.rules {
        let mut found = Vec::new();
        tables_in(language, vec![rule.body], &mut found);
        result.extend(found.into_iter().map(|table| (rule.name.as_str(), table)));
    }
    for expr in &language.exprs {
        let mut roots: Vec<NodeId> = expr
            .starts
            .iter()
            .filter_map(|start| match start {
                Start::Atom { body, .. } => Some(*body),
                Start::Prefix { .. } => None,
            })
            .collect();
        roots.extend(expr.continuations.iter().filter_map(|c| match c.operation {
            Operation::Postfix { body } => Some(body),
            Operation::Infix { .. } => None,
        }));
        let mut found = vec![&expr.table];
        tables_in(language, roots, &mut found);
        result.extend(found.into_iter().map(|table| (expr.name.as_str(), table)));
    }
    result
}

fn table_of(node: &Node) -> Option<&Table> {
    match node {
        Node::Choice { table, .. }
        | Node::Optional { table, .. }
        | Node::Many { table, .. }
        | Node::Separated { table, .. } => Some(table),
        _ => None,
    }
}

pub(super) fn report(language: &Language) -> Vec<Lookahead> {
    let mut found = Vec::new();
    for (rule, table) in tables(language) {
        let kinds = table
            .kinds
            .iter()
            .enumerate()
            .map(|(number, entry)| (language.kind_name(SyntaxKind(number as u16)), entry));
        let words = table
            .words
            .iter()
            .map(|(word, entry)| (word.as_str(), entry));
        for (start, entry) in kinds.chain(words) {
            if matches!(entry, Entry::Further(_)) {
                found.push(Lookahead {
                    rule: rule.to_owned(),
                    starts: start.to_owned(),
                    tokens: entry_depth(entry),
                });
            }
        }
    }
    found.sort_by(|a, b| (&a.rule, &a.starts).cmp(&(&b.rule, &b.starts)));
    found.dedup();
    found
}

pub(super) fn deepest(language: &Language) -> usize {
    tables(language)
        .into_iter()
        .map(|(_, table)| table_depth(table))
        .max()
        .unwrap_or(1)
}
