//! The form a grammar is run from: parts referring to each other by number, with
//! every decision worked out as a table and every recovery point as a set of kinds.

use std::sync::Arc;

use crate::{Assoc, SyntaxKind};

use super::builder::ParseFn;

pub(crate) type NodeId = usize;

/// A set of token kinds.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct KindSet(Vec<u64>);

impl KindSet {
    /// Kinds are numbered from zero; the two reserved ones that parsers see share the
    /// slots after the last.
    fn slot(kind: SyntaxKind) -> usize {
        if kind == SyntaxKind::EOF {
            0
        } else {
            usize::from(kind.0) + 1
        }
    }

    pub(crate) fn union(&mut self, other: &Self) {
        if self.0.len() < other.0.len() {
            self.0.resize(other.0.len(), 0);
        }
        for (word, more) in self.0.iter_mut().zip(&other.0) {
            *word |= more;
        }
    }

    pub(crate) fn insert(&mut self, kind: SyntaxKind) {
        let slot = Self::slot(kind);
        let word = slot / 64;
        if self.0.len() <= word {
            self.0.resize(word + 1, 0);
        }
        self.0[word] |= 1 << (slot % 64);
    }

    pub(crate) fn contains(&self, kind: SyntaxKind) -> bool {
        let slot = Self::slot(kind);
        self.0
            .get(slot / 64)
            .is_some_and(|word| word & (1 << (slot % 64)) != 0)
    }
}

/// Chooses between alternatives by looking at the next tokens.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Table {
    /// By kind number.
    pub(crate) kinds: Vec<Entry>,
    pub(crate) end: Entry,
    /// Contextual keywords, which are looked at before the identifier they are.
    pub(crate) words: Vec<(String, Entry)>,
    /// For tokens with no entry, when nothing else could be meant.
    pub(crate) otherwise: Option<u16>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) enum Entry {
    #[default]
    Miss,
    Take(u16),
    /// Look one token further.
    Further(Box<Table>),
}

/// In the tables of options and repetitions, the choice that enters them.
pub(crate) const ENTER: u16 = 0;
/// The choice that passes them by.
pub(crate) const PASS: u16 = 1;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) enum Node {
    Token(SyntaxKind),
    Contextual(SyntaxKind),
    /// Tokens with nothing between them that mean one token of `kind`.
    Glued {
        parts: Vec<SyntaxKind>,
        kind: SyntaxKind,
    },
    Rule(usize),
    Expr(usize),
    Extern(usize),
    Sequence(Vec<NodeId>),
    Choice {
        alternatives: Vec<NodeId>,
        /// Alternatives to try first, in order.
        attempts: Vec<NodeId>,
        table: Table,
        /// The alternative that can match nothing.
        empty: Option<NodeId>,
        /// The literals that start the alternatives, if all start with one.
        literals: Vec<SyntaxKind>,
    },
    Optional {
        body: NodeId,
        table: Table,
        /// The literal that starts the body, if one does.
        literal: Option<SyntaxKind>,
    },
    Many {
        body: NodeId,
        table: Table,
        /// Where reading can go on after unexpected text in the repetition: the
        /// recovery set worked out for it.
        stop: KindSet,
    },
    Separated {
        item: NodeId,
        separator: Symbol,
        strict: bool,
        table: Table,
        /// What can follow the list.
        follow: KindSet,
        /// Where reading can go on after the list ends because of something around
        /// it: the recovery set of the rule it is in, and what follows the list.
        outer: KindSet,
        /// Where reading can go on after unexpected text in the list: an item, a
        /// separator, or anything in `outer`.
        resume: KindSet,
        follow_literals: Vec<SyntaxKind>,
    },
    Terminated(NodeId),
    /// Parts between a pair of block delimiters: inside, a line ending ends only the
    /// statements written there.
    Block(NodeId),
    Look(NodeId),
    Not(NodeId),
    Attempt(NodeId),
    /// A part that could not be resolved. Grammars with one are refused.
    Missing,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Rule {
    pub(crate) name: String,
    /// `None` for a choice of rules, which adds no node.
    pub(crate) kind: Option<SyntaxKind>,
    pub(crate) body: NodeId,
    pub(crate) description: String,
}

/// The token or glued tokens of an operator.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Symbol {
    pub(crate) parts: Vec<SyntaxKind>,
    pub(crate) kind: SyntaxKind,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) enum Operation {
    Infix {
        assoc: Assoc,
    },
    /// The part parsed after the operand, which starts with the operator.
    Postfix {
        body: NodeId,
    },
}

/// An operator that continues an expression.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Continuation {
    pub(crate) symbol: Symbol,
    pub(crate) node: SyntaxKind,
    pub(crate) precedence: u16,
    pub(crate) operation: Operation,
}

/// What can start an operand.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) enum Start {
    Atom {
        node: SyntaxKind,
        body: NodeId,
    },
    Prefix {
        symbol: Symbol,
        node: SyntaxKind,
        precedence: u16,
    },
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Expr {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) starts: Vec<Start>,
    pub(crate) table: Table,
    /// Longest symbol first.
    pub(crate) continuations: Vec<Continuation>,
    /// By kind number, the continuations whose symbol starts with that kind.
    pub(crate) by_first: Vec<Vec<usize>>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct Extern {
    pub(crate) name: String,
    #[serde(with = "crate::embed::rule_code")]
    pub(crate) parse: Arc<ParseFn>,
    /// The tokens it can start with; empty if unknown.
    pub(crate) first: Vec<SyntaxKind>,
}

/// Something that can be referred to by name and parsed.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub(crate) enum Callee {
    Rule(usize),
    Expr(usize),
    Extern(usize),
}
