//! Where parsing resumes after text that nothing can use, worked out once when the
//! grammar is compiled.
//!
//! Every rule gets a set of token kinds: what can follow each rule that is sure to be
//! under way whenever this one is (its dominators in the graph of rules calling each
//! other), together with any kinds the grammar names for it. A repetition or a list
//! adds what it can start with and what follows it. At run time, recovery is asking
//! whether the current token is in one set: nothing is searched, built or allocated.

use super::analyze::{Input, bodies, children};
use super::ir::{Callee, KindSet, Node, NodeId};

/// The recovery set of every rule and expression, and which of them holds each part.
pub(super) struct Recovery {
    /// By part: the definition it belongs to, as a number from `Numbering`.
    holder: Vec<Option<usize>>,
    /// By definition.
    sets: Vec<KindSet>,
}

impl Recovery {
    /// Where parsing can resume inside the definition that holds `node`.
    pub(super) fn of(&self, node: NodeId) -> Option<&KindSet> {
        self.sets.get(self.holder.get(node).copied().flatten()?)
    }
}

/// Numbers every rule, expression and custom rule in turn.
struct Numbering {
    rules: usize,
    exprs: usize,
    externs: usize,
}

impl Numbering {
    fn of(input: &Input<'_>) -> Self {
        Self {
            rules: input.rules.len(),
            exprs: input.exprs.len(),
            externs: input.externs.len(),
        }
    }

    fn count(&self) -> usize {
        self.rules + self.exprs + self.externs
    }

    fn number(&self, callee: Callee) -> usize {
        match callee {
            Callee::Rule(index) => index,
            Callee::Expr(index) => self.rules + index,
            Callee::Extern(index) => self.rules + self.exprs + index,
        }
    }
}

/// Computes the recovery set of every definition.
///
/// `follow_rule` and `follow_expr` are what can follow each rule and expression, and
/// `named_rule` the kinds each rule says to resume at besides.
pub(super) fn compute(
    input: &Input<'_>,
    follow_rule: &[KindSet],
    follow_expr: &[KindSet],
    named_rule: &[KindSet],
) -> Recovery {
    let numbering = Numbering::of(input);
    let count = numbering.count();
    let mut holder = vec![None; input.nodes.len()];
    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); count];

    // Which definition calls which, and which holds each part.
    let mut bodies_of: Vec<(Callee, Vec<NodeId>)> = Vec::new();
    for (index, rule) in input.rules.iter().enumerate() {
        bodies_of.push((Callee::Rule(index), vec![rule.body]));
    }
    for (index, expr) in input.exprs.iter().enumerate() {
        bodies_of.push((Callee::Expr(index), bodies(expr).collect()));
    }
    for (callee, roots) in &bodies_of {
        let from = numbering.number(*callee);
        let mut pending = roots.clone();
        while let Some(node) = pending.pop() {
            holder[node] = Some(from);
            match &input.nodes[node] {
                Node::Rule(index) => edges[from].push(numbering.number(Callee::Rule(*index))),
                Node::Expr(index) => edges[from].push(numbering.number(Callee::Expr(*index))),
                Node::Extern(index) => edges[from].push(numbering.number(Callee::Extern(*index))),
                other => children(other, &mut pending),
            }
        }
    }
    // Custom code may call any rule by name, so it may lead anywhere.
    for index in 0..input.externs.len() {
        let from = numbering.number(Callee::Extern(index));
        edges[from] = (0..count).collect();
    }

    let dominators = dominators(&edges, numbering.number(input.root));
    let own = |number: usize| -> Option<&KindSet> {
        let (rules, exprs) = (numbering.rules, numbering.exprs);
        if number < rules {
            follow_rule.get(number)
        } else if number < rules + exprs {
            follow_expr.get(number - rules)
        } else {
            None
        }
    };
    let sets = (0..count)
        .map(|number| {
            let mut set = KindSet::default();
            for dominator in dominators.members(number) {
                if let Some(follow) = own(dominator) {
                    set.union(follow);
                }
                if let Some(named) = named_rule.get(dominator) {
                    set.union(named);
                }
            }
            set
        })
        .collect();
    Recovery { holder, sets }
}

/// For each node, the nodes that every path to it from the root goes through, itself
/// included. A node that cannot be reached has only itself.
struct Dominators {
    count: usize,
    words: usize,
    bits: Vec<u64>,
}

impl Dominators {
    fn row(&self, node: usize) -> &[u64] {
        let start = node * self.words;
        self.bits.get(start..start + self.words).unwrap_or_default()
    }

    fn members(&self, node: usize) -> impl Iterator<Item = usize> + '_ {
        let count = self.count;
        self.row(node)
            .iter()
            .enumerate()
            .flat_map(|(at, &word)| {
                (0..64)
                    .filter(move |bit| (word >> bit) & 1 == 1)
                    .map(move |bit| at * 64 + bit)
            })
            .filter(move |&member| member < count)
    }
}

fn dominators(edges: &[Vec<usize>], root: usize) -> Dominators {
    let count = edges.len();
    let words = count.div_ceil(64).max(1);
    let mut reached = vec![false; count];
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node < count && !std::mem::replace(&mut reached[node], true) {
            pending.extend(&edges[node]);
        }
    }
    let mut predecessors: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (from, targets) in edges.iter().enumerate().filter(|&(from, _)| reached[from]) {
        for &to in targets {
            predecessors[to].push(from);
        }
    }

    // Start from "everything dominates", and shrink to what all the ways in share.
    let mut bits = vec![u64::MAX; count * words];
    let set_only = |bits: &mut [u64], node: usize| {
        bits[node * words..(node + 1) * words].fill(0);
        bits[node * words + node / 64] |= 1 << (node % 64);
    };
    for (node, &is_reached) in reached.iter().enumerate() {
        if node == root || !is_reached {
            set_only(&mut bits, node);
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for node in (0..count).filter(|&node| node != root && reached[node]) {
            let mut meet = vec![u64::MAX; words];
            for &before in &predecessors[node] {
                for (word, &other) in meet
                    .iter_mut()
                    .zip(&bits[before * words..(before + 1) * words])
                {
                    *word &= other;
                }
            }
            meet[node / 64] |= 1 << (node % 64);
            let row = &mut bits[node * words..(node + 1) * words];
            if row != meet.as_slice() {
                row.copy_from_slice(&meet);
                changed = true;
            }
        }
    }
    Dominators { count, words, bits }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn members(edges: &[Vec<usize>], root: usize) -> Vec<Vec<usize>> {
        let found = dominators(edges, root);
        (0..edges.len())
            .map(|node| found.members(node).collect())
            .collect()
    }

    #[test]
    fn a_chain_is_dominated_by_everything_before() {
        let edges = vec![vec![1], vec![2], vec![]];
        assert_eq!(members(&edges, 0), [vec![0], vec![0, 1], vec![0, 1, 2]]);
    }

    #[test]
    fn only_what_every_path_goes_through_dominates() {
        // 0 -> 1 -> 3 and 0 -> 2 -> 3: neither 1 nor 2 is on every path to 3.
        let edges = vec![vec![1, 2], vec![3], vec![3], vec![]];
        assert_eq!(
            members(&edges, 0),
            [vec![0], vec![0, 1], vec![0, 2], vec![0, 3]]
        );
    }

    #[test]
    fn cycles_and_unreachable_nodes_are_handled() {
        // 0 -> 1 -> 2 -> 1, and 3 is never reached.
        let edges = vec![vec![1], vec![2], vec![1], vec![1]];
        assert_eq!(
            members(&edges, 0),
            [vec![0], vec![0, 1], vec![0, 1, 2], vec![3]]
        );
    }

    #[test]
    fn many_nodes_need_more_than_one_word() {
        let count = 130;
        let edges: Vec<Vec<usize>> = (0..count)
            .map(|node| {
                if node + 1 < count {
                    vec![node + 1]
                } else {
                    vec![]
                }
            })
            .collect();
        let found = members(&edges, 0);
        assert_eq!(found[129].len(), 130);
        assert_eq!(found[129].last(), Some(&129));
    }
}
