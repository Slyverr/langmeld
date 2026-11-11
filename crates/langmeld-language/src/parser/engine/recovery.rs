use langmeld_core::{Diagnostic, Span};

use super::super::pattern::any_construct_matches;
use super::Parser;
use crate::ConstructSpec;

impl Parser<'_> {
    pub(super) fn should_resume_at_root(&mut self, allowed: &[&ConstructSpec]) -> bool {
        self.should_resume_at_root_at(allowed, self.cursor)
    }

    pub(super) fn should_resume_at_root_at(
        &mut self,
        allowed: &[&ConstructSpec],
        position: usize,
    ) -> bool {
        if !self.recover_at_root_boundaries {
            return false;
        }

        let root_matches = match any_construct_matches(
            &self.root_constructs,
            &self.tokens,
            position,
            &mut self.pattern_steps_remaining,
        ) {
            Ok(matches) => matches,
            Err(()) => {
                self.stop_for_pattern_limit_at(position);
                return false;
            }
        };
        if !root_matches {
            return false;
        }

        let child_matches = match any_construct_matches(
            allowed,
            &self.tokens,
            position,
            &mut self.pattern_steps_remaining,
        ) {
            Ok(matches) => matches,
            Err(()) => {
                self.stop_for_pattern_limit_at(position);
                return false;
            }
        };
        !child_matches
    }

    pub(super) fn missing_block_open(&mut self, spec: &ConstructSpec, open: &str) {
        self.completion_blocked = true;
        let span = self.tokens.get(self.cursor).map_or_else(
            || Span::new(self.source.text().len(), self.source.text().len()),
            |token| token.span,
        );
        self.diagnostics.push(Diagnostic::error(
            "LM-PARSE-005",
            format!(
                "expected opening token `{open}` after construct `{}`",
                spec.name
            ),
            self.source,
            span,
        ));
    }

    pub(super) fn stop_for_pattern_limit(&mut self) {
        self.stop_for_pattern_limit_at(self.cursor);
    }

    pub(super) fn stop_for_pattern_limit_at(&mut self, position: usize) {
        self.resource_limited = true;
        self.stopped = true;
        let span = self.tokens.get(position).map_or_else(
            || Span::new(self.source.text().len(), self.source.text().len()),
            |token| token.span,
        );
        self.diagnostics.push(Diagnostic::error(
            "LM-PARSE-010",
            format!(
                "pattern matching exceeds the configured limit of {} steps",
                self.limits.max_pattern_steps
            ),
            self.source,
            span,
        ));
    }

    pub(super) fn skip_block(&mut self, open: &str, close: &str) {
        let mut nesting = 0usize;
        while let Some(token) = self.tokens.get(self.cursor) {
            if token.text == open {
                nesting += 1;
            } else if token.text == close {
                nesting -= 1;
                self.cursor += 1;
                if nesting == 0 {
                    return;
                }
                continue;
            }
            self.cursor += 1;
        }
    }
}
