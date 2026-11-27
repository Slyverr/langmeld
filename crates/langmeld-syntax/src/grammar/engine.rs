//! Runs a grammar on the parser primitives. Nothing here uses more of the [`Parser`]
//! than a parser written by hand could.
//!
//! Every decision is one lookup in a table worked out beforehand, and every place to
//! resume after unexpected text is a set of kinds worked out beforehand, so the work
//! of parsing does not depend on how many rules the grammar has.

use std::cell::Cell;

use crate::lexeme::Vocabulary;
use crate::parser::Rules;
use crate::{Parser, SyntaxKind, TextSize};

use super::Assoc;
use super::Language;
use super::ir::{Callee, ENTER, Entry, KindSet, Node, NodeId, Operation, Start, Symbol, Table};

/// One parse under way. Custom code that calls back into rules reaches this same
/// engine through the parser, so what is under way is never lost.
pub(super) struct Engine<'l> {
    language: &'l Language,
    /// How many statements that a line can end are under way, since the last block.
    statements: Cell<usize>,
    /// For each rule and expression under way, where it was last entered.
    rules: Vec<Cell<Option<TextSize>>>,
    exprs: Vec<Cell<Option<TextSize>>>,
}

/// Records, at the end of the text, every token the table has a way for.
fn offer(language: &Language, table: &Table, p: &mut Parser<'_>) {
    let kinds = table.kinds.iter().zip(0..);
    for (entry, kind) in kinds {
        if !matches!(entry, Entry::Miss) {
            p.may_follow(SyntaxKind(kind));
        }
    }
    for (word, _) in &table.words {
        if let Some(&kind) = language.words.get(word) {
            p.may_follow(kind);
        }
    }
}

/// The alternative the next tokens call for.
fn decide(language: &Language, table: &Table, p: &mut Parser<'_>) -> Option<u16> {
    let mut table = table;
    let mut ahead = 0;
    loop {
        let kind = p.nth(ahead);
        let word = if table.words.is_empty() || !language.lexer.is_identifier(kind) {
            None
        } else {
            let text = p.nth_text(ahead);
            table.words.iter().find(|(word, _)| word == text)
        };
        if kind == SyntaxKind::EOF {
            offer(language, table, p);
        }
        let entry = match word {
            Some((_, entry)) => Some(entry),
            None if kind == SyntaxKind::EOF => Some(&table.end),
            None => table.kinds.get(usize::from(kind.0)),
        };
        match entry {
            Some(Entry::Take(choice)) => return Some(*choice),
            Some(Entry::Further(further)) => {
                table = further;
                ahead += 1;
            }
            Some(Entry::Miss) | None => return table.otherwise,
        }
    }
}

impl Rules for Engine<'_> {
    fn parse(&self, name: &str, p: &mut Parser<'_>) -> bool {
        let Some(&callee) = self.language.callees.get(name) else {
            return false;
        };
        self.call(callee, p);
        true
    }

    fn kind(&self, name: &str) -> Option<SyntaxKind> {
        self.language.kind(name)
    }
}

impl<'l> Engine<'l> {
    pub(super) fn new(language: &'l Language) -> Self {
        Self {
            language,
            statements: Cell::new(0),
            rules: vec![Cell::new(None); language.rules.len()],
            exprs: vec![Cell::new(None); language.exprs.len()],
        }
    }

    /// Whether `set` holds the current token, or the contextual word that the current
    /// identifier spells.
    fn holds(&self, set: &KindSet, p: &mut Parser<'_>) -> bool {
        let current = p.current();
        if set.contains(current) {
            return true;
        }
        let language = self.language;
        !language.words.is_empty()
            && language.lexer.is_identifier(current)
            && language
                .words
                .get(p.current_text())
                .is_some_and(|&word| set.contains(word))
    }

    fn name(&self, kind: SyntaxKind) -> &'l str {
        self.language.kind_name(kind)
    }

    /// Parses a whole file.
    pub(super) fn file(&self, p: &mut Parser<'_>) {
        match self.language.root {
            Callee::Rule(index) => {
                let rule = &self.language.rules[index];
                let node = p.start();
                self.rules[index].set(Some(p.current_range().start()));
                self.exec(rule.body, &rule.description, p);
                self.skip_lines(p);
                // Whatever is left is reported and kept inside the root.
                if !p.at_end() {
                    p.recover("the end of the file", &[]);
                }
                match rule.kind {
                    Some(kind) => node.complete(p, kind),
                    None => node.complete(p, SyntaxKind::ERROR),
                };
            }
            callee => {
                self.call(callee, p);
            }
        }
    }

    /// Parses what `callee` names. Returns false if what it must start with was
    /// missing.
    fn call(&self, callee: Callee, p: &mut Parser<'_>) -> bool {
        let language = self.language;
        match callee {
            Callee::Rule(index) => {
                // With a token missing, parsing goes on to what follows, which may be
                // the same rule again. Entering it where it already is would never
                // end.
                let here = p.current_range().start();
                if self.rules[index].get() == Some(here) {
                    return false;
                }
                let outer = self.rules[index].replace(Some(here));
                let rule = &language.rules[index];
                let found = match rule.kind {
                    Some(kind) => {
                        let node = p.start();
                        let found = self.exec(rule.body, &rule.description, p);
                        node.complete(p, kind);
                        found
                    }
                    None => self.exec(rule.body, &rule.description, p),
                };
                self.rules[index].set(outer);
                found
            }
            Callee::Expr(index) => self.expression(index, 0, p),
            Callee::Extern(index) => {
                let before = p.current_range().start();
                (language.externs[index].parse)(p);
                p.current_range().start() != before
            }
        }
    }

    /// Where line endings terminate, passes over those that nothing asks for.
    fn skip_lines(&self, p: &mut Parser<'_>) {
        if let Some(newline) = self.language.newline {
            while p.current() == newline {
                p.bump_skipped();
            }
        }
    }

    /// Passes over line breaks where there is no statement for them to end.
    fn skip_free_lines(&self, p: &mut Parser<'_>) {
        if self.statements.get() == 0 {
            self.skip_lines(p);
        }
    }

    fn skip_lines_unless(&self, table: &Table, p: &mut Parser<'_>) {
        if let Some(newline) = self.language.newline {
            let wanted = !matches!(
                table.kinds.get(usize::from(newline.0)),
                None | Some(Entry::Miss)
            );
            if !wanted {
                self.skip_lines(p);
            }
        }
    }

    /// Another word where a word of the language belongs was most likely meant as
    /// it, so it is taken in its place and parsing stays in step.
    fn take_wrong_word(&self, p: &mut Parser<'_>) {
        let current = p.current();
        if self.language.lexer.is_identifier(current) && !p.on_new_line() {
            let wrong = p.start();
            p.bump_skipped();
            wrong.complete(p, SyntaxKind::ERROR);
        }
    }

    /// Parses one part. `context` describes the rule it is in.
    fn exec(&self, node: NodeId, context: &'l str, p: &mut Parser<'_>) -> bool {
        let language = self.language;
        match &language.nodes[node] {
            &Node::Token(kind) => {
                if language.newline != Some(kind) {
                    self.skip_lines(p);
                }
                let found = p.expect(kind);
                if !found && language.lexer.is_keyword(kind) {
                    self.take_wrong_word(p);
                }
                found
            }
            &Node::Contextual(kind) => {
                self.skip_lines(p);
                let found = p.eat_contextual(kind);
                if !found {
                    p.error_expected();
                    self.take_wrong_word(p);
                }
                found
            }
            Node::Glued { parts, kind } => {
                self.skip_lines(p);
                let found = p.at_glued(parts);
                if found {
                    p.bump_glued(*kind, parts.len());
                } else {
                    p.expected_with(|| format!("`{}`", self.name(*kind)));
                }
                found
            }
            &Node::Rule(index) => self.call(Callee::Rule(index), p),
            &Node::Expr(index) => self.call(Callee::Expr(index), p),
            &Node::Extern(index) => self.call(Callee::Extern(index), p),
            Node::Sequence(parts) => {
                for (position, &part) in parts.iter().enumerate() {
                    let found = self.exec(part, context, p);
                    // Without its first part, a rule has not begun; looking for the
                    // rest would only mislead.
                    if !found && position == 0 {
                        return false;
                    }
                }
                true
            }
            Node::Block(body) => {
                // Inside, line breaks end only the statements written there.
                let outer = self.statements.replace(0);
                let found = self.exec(*body, context, p);
                self.statements.set(outer);
                found
            }
            Node::Choice {
                alternatives,
                attempts,
                table,
                empty,
                literals,
            } => {
                self.skip_lines_unless(table, p);
                for &attempted in attempts {
                    if p.attempt(|p| {
                        self.exec(attempted, context, p);
                    }) {
                        return true;
                    }
                }
                match decide(language, table, p) {
                    Some(choice) => self.exec(alternatives[usize::from(choice)], context, p),
                    None => match empty {
                        Some(part) => self.exec(*part, context, p),
                        None => {
                            if literals.is_empty() {
                                p.expected(context);
                            } else {
                                for &literal in literals {
                                    p.at(literal);
                                }
                                p.error_expected();
                            }
                            false
                        }
                    },
                }
            }
            Node::Optional {
                body,
                table,
                literal,
            } => {
                self.skip_free_lines(p);
                if decide(language, table, p) == Some(ENTER) {
                    self.exec(*body, context, p);
                } else if let Some(literal) = *literal {
                    // So that a later error can list it.
                    p.at(literal);
                }
                true
            }
            Node::Many { body, table, stop } => {
                self.repeat(*body, table, stop, context, p);
                true
            }
            Node::Separated {
                item,
                separator,
                strict,
                table,
                follow,
                outer,
                resume,
                follow_literals,
            } => {
                loop {
                    self.skip_free_lines(p);
                    if decide(language, table, p) != Some(ENTER) {
                        // The list ends here, unless this is text nothing can use.
                        let ends = p.at_end() || self.holds(follow, p) || self.holds(outer, p);
                        if ends || !self.skip_in_list(*item, resume, context, p) {
                            break;
                        }
                        continue;
                    }
                    let before = p.current_range().start();
                    self.exec(*item, context, p);
                    if p.current_range().start() == before {
                        break;
                    }
                    if Self::eat(separator, p) {
                        if *strict && decide(language, table, p) != Some(ENTER) {
                            self.exec(*item, context, p);
                            break;
                        }
                        continue;
                    }
                    if self.holds(follow, p) {
                        break;
                    }
                    if decide(language, table, p) == Some(ENTER) {
                        // A separator is missing, and another item follows.
                        for &literal in follow_literals {
                            p.at(literal);
                        }
                        p.error_expected();
                    }
                }
                true
            }
            &Node::Terminated(body) => {
                let outer = self.statements.get();
                self.statements.set(outer.saturating_add(1));
                let found = self.exec(body, context, p);
                self.statements.set(outer);
                if found {
                    self.terminator(p);
                }
                found
            }
            &Node::Look(body) => {
                let found = p.lookahead(|p| {
                    self.exec(body, context, p);
                });
                if !found {
                    p.expected(context);
                }
                found
            }
            &Node::Not(body) => {
                let found = p.lookahead(|p| {
                    self.exec(body, context, p);
                });
                if found {
                    p.expected(context);
                }
                !found
            }
            &Node::Attempt(body) => {
                p.attempt(|p| {
                    self.exec(body, context, p);
                });
                true
            }
            Node::Missing => false,
        }
    }

    fn repeat(
        &self,
        body: NodeId,
        table: &'l Table,
        stop: &KindSet,
        context: &'l str,
        p: &mut Parser<'_>,
    ) {
        let language = self.language;
        let lines = matches!(language.nodes[body], Node::Terminated(_));
        loop {
            if lines {
                self.skip_lines(p);
            } else {
                self.skip_free_lines(p);
            }
            if p.at_end() {
                offer(language, table, p);
                return;
            }
            let before = p.current_range().start();
            if decide(language, table, p) == Some(ENTER) {
                self.exec(body, context, p);
                if p.current_range().start() != before {
                    continue;
                }
            }
            if self.holds(stop, p) {
                return;
            }
            // Nothing under way can go on from here: skip to where something can.
            p.recover_until(self.description(body, context), |p| self.holds(stop, p));
            if p.current_range().start() == before {
                return;
            }
        }
    }

    /// Skips unexpected text in a list, up to where the list or something around it
    /// can go on. Returns false if nothing was skipped.
    fn skip_in_list(
        &self,
        item: NodeId,
        resume: &KindSet,
        context: &'l str,
        p: &mut Parser<'_>,
    ) -> bool {
        if self.holds(resume, p) {
            return false;
        }
        let before = p.current_range().start();
        p.recover_until(self.description(item, context), |p| self.holds(resume, p));
        p.current_range().start() != before
    }

    /// How an error names a part.
    fn description(&self, node: NodeId, context: &'l str) -> &'l str {
        let language = self.language;
        match &language.nodes[node] {
            &Node::Rule(index) => &language.rules[index].description,
            &Node::Expr(index) => &language.exprs[index].description,
            &Node::Terminated(body) | &Node::Attempt(body) => self.description(body, context),
            &Node::Token(kind) => self.name(kind),
            _ => context,
        }
    }

    fn terminator(&self, p: &mut Parser<'_>) {
        let language = self.language;
        let Some(newline) = language.newline else {
            return;
        };
        let current = p.current();
        if current == newline {
            p.bump();
            return;
        }
        for symbol in &language.terminators {
            if Self::found(symbol, current, p) {
                Self::operator(symbol, p);
                return;
            }
        }
        // A block may close without one.
        if current != SyntaxKind::EOF && !language.closers.contains(current) {
            p.at(newline);
            for symbol in &language.terminators {
                if let Some(&first) = symbol.parts.first() {
                    p.at(first);
                }
            }
            p.error_expected();
        }
    }

    /// Takes a separator if it is next, noting that it was looked for if not.
    fn eat(symbol: &Symbol, p: &mut Parser<'_>) -> bool {
        let current = p.current();
        if Self::found(symbol, current, p) {
            Self::operator(symbol, p);
            return true;
        }
        if let Some(&first) = symbol.parts.first() {
            p.at(first);
        }
        false
    }

    /// Takes the token or glued tokens of an operator.
    fn operator(symbol: &Symbol, p: &mut Parser<'_>) {
        if symbol.parts.len() > 1 {
            p.bump_glued(symbol.kind, symbol.parts.len());
        } else {
            p.bump();
        }
    }

    fn found(symbol: &Symbol, current: SyntaxKind, p: &mut Parser<'_>) -> bool {
        symbol.parts.first() == Some(&current)
            && (symbol.parts.len() == 1 || p.at_glued(&symbol.parts))
    }

    /// Parses operands joined by operators that bind tighter than `above`.
    fn expression(&self, index: usize, above: u16, p: &mut Parser<'_>) -> bool {
        self.skip_lines(p);
        let here = p.current_range().start();
        if self.exprs[index].get() == Some(here) {
            return false;
        }
        let outer = self.exprs[index].replace(Some(here));
        let found = self.operations(index, above, p);
        self.exprs[index].set(outer);
        found
    }

    fn operations(&self, index: usize, above: u16, p: &mut Parser<'_>) -> bool {
        let language = self.language;
        let expr = &language.exprs[index];
        let Some(start) = decide(language, &expr.table, p) else {
            p.expected(&expr.description);
            return false;
        };
        let marker = p.start();
        let mut left = match &expr.starts[usize::from(start)] {
            Start::Atom { node, body } => {
                self.exec(*body, &expr.description, p);
                marker.complete(p, *node)
            }
            Start::Prefix {
                symbol,
                node,
                precedence,
            } => {
                let current = p.current();
                if !Self::found(symbol, current, p) {
                    marker.abandon(p);
                    p.expected(&expr.description);
                    return false;
                }
                Self::operator(symbol, p);
                self.expression(index, *precedence, p);
                marker.complete(p, *node)
            }
        };
        // The precedence of the last operator here that does not chain.
        let mut unchainable = None;
        loop {
            self.skip_free_lines(p);
            let current = p.current();
            if current == SyntaxKind::EOF {
                for continuation in &expr.continuations {
                    if let Some(&first) = continuation.symbol.parts.first() {
                        p.may_follow(first);
                    }
                }
            }
            let Some(next) = expr
                .by_first
                .get(usize::from(current.0))
                .into_iter()
                .flatten()
                .map(|&position| &expr.continuations[position])
                .find(|c| Self::found(&c.symbol, current, p))
            else {
                break;
            };
            if next.precedence <= above {
                break;
            }
            if let Operation::Infix { assoc: Assoc::None } = next.operation {
                if unchainable == Some(next.precedence) {
                    p.error_with("LM-P006", || {
                        format!(
                            "`{}` cannot be chained; add parentheses around one of the operations",
                            self.name(next.symbol.kind)
                        )
                    });
                }
                unchainable = Some(next.precedence);
            }
            let marker = left.precede(p);
            match next.operation {
                Operation::Infix { assoc } => {
                    Self::operator(&next.symbol, p);
                    // An operator that groups to the right lets itself bind again.
                    let binding = match assoc {
                        Assoc::Right => next.precedence.saturating_sub(1),
                        Assoc::Left | Assoc::None => next.precedence,
                    };
                    self.expression(index, binding, p);
                }
                Operation::Postfix { body } => {
                    self.exec(body, &expr.description, p);
                }
            }
            left = marker.complete(p, next.node);
        }
        true
    }
}
