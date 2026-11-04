//! Candidate selection, AST construction, and recoverable block parsing.

use std::collections::{BTreeMap, BTreeSet};

use langmeld_core::{
    BlockSeparatorPolicy, ConstructSpec, Diagnostic, FieldKind, FieldSpec, MapKeyKind, SourceFile,
    Span,
};

use crate::{
    AstNode, BlockDelimiterSpan, CapturedValue, CompletionBlockContext, CompletionFrontier,
    CustomConstructMatch, CustomConstructParser, ExpectedCapture, ExpressionAtomParserRegistration,
    LanguageToken, LanguageTokenKind as TokenKind, ParseLimits, Program, Value, ValueType,
};

use super::{
    candidates::BlockCandidateIndex,
    matching::{PatternMatcher, PatternProgress, default_value},
    prepared::{CompiledBlock, CompiledBlockSeparator, ConfiguredLanguageParser, PreparedLanguage},
};

const MAX_COMPLETION_FRONTIERS: usize = 1024;
const MAX_COMPLETION_BLOCKS: usize = 1024;
const MAX_COMPLETION_EXPECTED_CAPTURES: usize = 64;

pub(super) struct ParsedTokens {
    pub(super) program: Program,
    pub(super) diagnostics: Vec<Diagnostic>,
    pub(super) custom_parser_spans: Vec<Span>,
    pub(super) block_delimiters: Vec<BlockDelimiterSpan>,
    pub(super) completion_frontiers: Vec<CompletionFrontier>,
    pub(super) completion_blocks: Vec<CompletionBlockContext>,
    pub(super) resource_limited: bool,
}

pub(super) fn parse_tokens(
    parser: &ConfiguredLanguageParser,
    source: &SourceFile,
    tokens: Vec<LanguageToken>,
    limits: ParseLimits,
) -> ParsedTokens {
    Parser {
        language: &parser.language,
        source,
        tokens,
        custom_parsers: &parser.parsers,
        expression_atom_parsers: &parser.expression_atom_parsers,
        position: 0,
        diagnostics: Vec::new(),
        custom_parser_spans: Vec::new(),
        block_delimiters: Vec::new(),
        completion_frontiers: Vec::new(),
        completion_blocks: Vec::new(),
        depth: 0,
        ast_node_count: 0,
        limits,
        remaining_pattern_steps: limits.max_pattern_steps,
        resource_limited: false,
    }
    .parse()
}

struct Parser<'a> {
    language: &'a PreparedLanguage,
    source: &'a SourceFile,
    tokens: Vec<LanguageToken>,
    custom_parsers: &'a [CustomConstructParser],
    expression_atom_parsers: &'a BTreeMap<String, ExpressionAtomParserRegistration>,
    position: usize,
    diagnostics: Vec<Diagnostic>,
    custom_parser_spans: Vec<Span>,
    block_delimiters: Vec<BlockDelimiterSpan>,
    completion_frontiers: Vec<CompletionFrontier>,
    completion_blocks: Vec<CompletionBlockContext>,
    depth: usize,
    ast_node_count: usize,
    limits: ParseLimits,
    remaining_pattern_steps: usize,
    resource_limited: bool,
}

struct Candidate {
    construct_index: usize,
    fields: BTreeMap<String, CapturedValue>,
    end_position: usize,
    syntax_variant: String,
    span: Span,
    custom_children: Option<Vec<AstNode>>,
    diagnostics: Vec<Diagnostic>,
    host_data: Option<crate::HostData>,
    custom: bool,
}

impl Parser<'_> {
    fn parse(mut self) -> ParsedTokens {
        self.skip_separators();
        let root = self.language.spec.spec().root.clone();
        let nodes =
            if let Some(node) = self.parse_construct(std::slice::from_ref(&root), true, None) {
                self.skip_separators();
                if !matches!(self.current().kind, TokenKind::End) {
                    self.diagnostics.push(Diagnostic::error(
                        "LM1201",
                        "unexpected content after the root construct",
                        self.current().span,
                    ));
                }
                vec![node]
            } else {
                Vec::new()
            };
        let (completion_frontiers, completion_blocks) = if self.resource_limited {
            (Vec::new(), Vec::new())
        } else {
            (self.completion_frontiers, self.completion_blocks)
        };
        ParsedTokens {
            program: Program { nodes },
            diagnostics: self.diagnostics,
            custom_parser_spans: self.custom_parser_spans,
            block_delimiters: self.block_delimiters,
            completion_frontiers,
            completion_blocks,
            resource_limited: self.resource_limited,
        }
    }

    fn parse_construct(
        &mut self,
        allowed: &[String],
        is_root: bool,
        candidate_index: Option<&BlockCandidateIndex>,
    ) -> Option<AstNode> {
        let start_position = self.position;
        let token_kind = self.current().kind.clone();
        let candidates = Self::candidate_ids(allowed, &token_kind, candidate_index);
        let mut best: Option<Candidate> = None;
        let mut tied = false;
        let mut best_value_error = None;
        let mut custom_error = None;
        let mut furthest_progress: Option<(usize, CompletionFrontier)> = None;

        for construct_id in candidates {
            let Some(compiled) = self.language.compiled.get(construct_id) else {
                continue;
            };
            let Some(construct) = self
                .language
                .spec
                .spec()
                .constructs
                .get(compiled.construct_index)
            else {
                continue;
            };
            if let Some(parser_id) = &construct.custom_parser
                && let Some(callback) = self
                    .custom_parsers
                    .iter()
                    .find(|callback| callback.id == *parser_id)
            {
                match (callback.parse)(self.source, self.current().span.start) {
                    Ok(Some(parsed)) => {
                        match self.validate_custom_match(construct, parsed, start_position) {
                            Ok(candidate) => self.consider(&mut best, &mut tied, candidate),
                            Err(diagnostic) => custom_error = Some(diagnostic),
                        }
                    }
                    Ok(None) => {}
                    Err(diagnostic) => {
                        if valid_diagnostic(self.source, &diagnostic) {
                            custom_error = Some(diagnostic);
                            self.custom_parser_spans.push(self.current_line_span());
                        } else {
                            custom_error =
                                Some(invalid_custom_match(self.current().span, parser_id));
                        }
                    }
                }
            }

            for pattern in &compiled.patterns {
                let mut matcher = PatternMatcher::new(
                    self.source,
                    &self.tokens,
                    &mut self.remaining_pattern_steps,
                )
                .with_expressions(&self.language.expressions)
                .with_lexical_spec(self.language.spec.spec().lexical.as_ref())
                .with_expression_atom_parsers(
                    &self.language.expression_atom_parser_ids,
                    self.expression_atom_parsers,
                );
                let matched = matcher.match_pattern(&pattern.parts, start_position);
                let pattern_progress = matcher.progress().cloned();
                let matched = match matched {
                    Ok(value) => value,
                    Err(diagnostic) if diagnostic.code == "LM1304" => {
                        self.resource_limited = true;
                        self.diagnostics.push(diagnostic);
                        self.recover_line();
                        return None;
                    }
                    Err(diagnostic) => {
                        merge_completion_frontier(
                            &mut furthest_progress,
                            &construct.id,
                            &pattern.variant_id,
                            pattern_progress,
                        );
                        if best_value_error.is_none() {
                            best_value_error = Some(diagnostic);
                        }
                        continue;
                    }
                };
                let Some(mut matched) = matched else {
                    merge_completion_frontier(
                        &mut furthest_progress,
                        &construct.id,
                        &pattern.variant_id,
                        pattern_progress,
                    );
                    continue;
                };
                if let Err(diagnostic) = matched.normalize_record_captures(construct) {
                    if best_value_error.is_none() {
                        best_value_error = Some(diagnostic);
                    }
                    continue;
                }
                let block_opener = construct.block.is_some()
                    && (self.language.spec.spec().schema_version >= 2
                        || (matched.end_position > start_position
                            && is_punct(&self.tokens[matched.end_position - 1].kind, '{')));
                if !is_line_end(&self.tokens, matched.end_position) && !block_opener {
                    continue;
                }
                apply_defaults(
                    construct,
                    &mut matched.fields,
                    self.tokens[start_position].span.start,
                    &mut best_value_error,
                );
                let candidate = Candidate {
                    construct_index: compiled.construct_index,
                    fields: matched.fields,
                    end_position: matched.end_position,
                    syntax_variant: pattern.variant_id.clone(),
                    span: self.range(start_position, matched.end_position),
                    custom_children: None,
                    diagnostics: Vec::new(),
                    host_data: None,
                    custom: false,
                };
                self.consider(&mut best, &mut tied, candidate);
            }
        }

        let Some(mut candidate) = best else {
            let custom_parser_limited = custom_error
                .as_ref()
                .is_some_and(|diagnostic| diagnostic.code == "LM1304");
            if !self.resource_limited
                && !custom_parser_limited
                && let Some((_, frontier)) = furthest_progress
            {
                self.record_completion_frontier(frontier);
            }
            if let Some(diagnostic) = custom_error.or(best_value_error) {
                self.resource_limited |= diagnostic.code == "LM1302";
                self.diagnostics.push(diagnostic);
            } else if is_root {
                self.diagnostics.push(Diagnostic::error(
                    "LM1200",
                    format!(
                        "expected root construct `{}`",
                        self.language.spec.spec().root
                    ),
                    self.current().span,
                ));
            } else if !is_root {
                self.diagnostics.push(Diagnostic::error(
                    "LM1202",
                    format!(
                        "no construct matches here; expected one of: {}",
                        allowed.join(", ")
                    ),
                    self.current().span,
                ));
            }
            self.recover_line();
            return None;
        };
        if tied {
            self.diagnostics.push(Diagnostic::error(
                "LM1203",
                "input matches more than one construct pattern; make the syntax unambiguous",
                self.tokens[start_position].span,
            ));
            if !is_root {
                self.recover_line();
            }
            return None;
        }

        let construct = &self.language.spec.spec().constructs[candidate.construct_index];
        self.validate_constraints(construct, &candidate.fields);
        let block_opener = construct.block.is_some()
            && (self.language.spec.spec().schema_version >= 2
                || (candidate.end_position > start_position
                    && is_punct(&self.tokens[candidate.end_position - 1].kind, '{')));
        if is_root && !is_line_end(&self.tokens, candidate.end_position) && !block_opener {
            self.diagnostics.push(Diagnostic::error(
                "LM1200",
                format!("expected root construct `{}`", construct.id),
                self.current().span,
            ));
            self.recover_line();
            return None;
        }
        if !self.count_node(candidate.span) {
            self.recover_line();
            return None;
        }
        self.position = candidate.end_position;
        if candidate.custom {
            self.custom_parser_spans.push(candidate.span);
        }
        self.diagnostics.append(&mut candidate.diagnostics);
        let has_custom_children = candidate.custom_children.is_some();
        let mut node = AstNode {
            construct: construct.id.clone(),
            syntax_variant: candidate.syntax_variant,
            fields: candidate.fields,
            children: candidate.custom_children.unwrap_or_default(),
            span: candidate.span,
            host_data: candidate.host_data,
        };

        if let Some(block) = &construct.block
            && !has_custom_children
        {
            if let Some(layout) = self.language.blocks.get(&construct.id)
                && let Some(start) =
                    self.sequence_start_ending_at(candidate.end_position, &layout.open)
                && let (Some(first), Some(last)) = (
                    self.tokens.get(start),
                    self.tokens.get(candidate.end_position.saturating_sub(1)),
                )
            {
                self.block_delimiters.push(BlockDelimiterSpan {
                    construct: construct.id.clone(),
                    span: Span::new(first.span.start, last.span.end),
                    opens: true,
                });
            }
            self.skip_separators();
            if self.depth >= self.max_nesting_depth() {
                self.resource_limited = true;
                self.diagnostics.push(Diagnostic::error(
                    "LM1205",
                    format!(
                        "block nesting exceeds the limit of {}",
                        self.max_nesting_depth()
                    ),
                    node.span,
                ));
                if let Some(close) = self.skip_balanced_block(&construct.id) {
                    node.span.end = close;
                } else {
                    node.span.end = self.source.text.len();
                }
            } else {
                self.depth += 1;
                let open_end = self
                    .tokens
                    .get(candidate.end_position.saturating_sub(1))
                    .map_or(candidate.span.end, |token| token.span.end);
                let children = self.parse_children(&construct.id, &block.allowed, open_end);
                self.depth -= 1;
                node.children = children.0;
                if let Some(close) = children.1 {
                    node.span.end = close;
                } else {
                    node.span.end = self.source.text.len();
                }
            }
        }
        if has_custom_children {
            let mut retained = Vec::new();
            for child in node.children {
                let Some(child) = self.retain_custom_tree(child) else {
                    break;
                };
                retained.push(child);
            }
            node.children = retained;
            for child in &node.children {
                self.validate_custom_tree_constraints(child);
            }
        }
        Some(node)
    }

    fn record_completion_frontier(&mut self, frontier: CompletionFrontier) {
        if let Some(existing) = self
            .completion_frontiers
            .iter_mut()
            .find(|existing| existing.span == frontier.span)
        {
            if existing.continuation != frontier.continuation {
                existing.continuation = None;
            }
            if existing.ambiguous || frontier.ambiguous {
                existing.ambiguous = true;
                return;
            }
            for capture in &frontier.captures {
                if !existing.captures.iter().any(|current| {
                    current.construct == capture.construct
                        && current.field == capture.field
                        && current.kind == capture.kind
                }) {
                    existing.ambiguous = true;
                    return;
                }
            }
            existing.captures.extend(frontier.captures);
        } else if self.completion_frontiers.len() < MAX_COMPLETION_FRONTIERS {
            self.completion_frontiers.push(frontier);
        }
    }

    fn record_completion_block(&mut self, parent: &str, allowed: &[String], span: Span) {
        if self.completion_blocks.len() >= MAX_COMPLETION_BLOCKS
            || span.start > span.end
            || !self.source.text.is_char_boundary(span.start)
            || !self.source.text.is_char_boundary(span.end)
        {
            return;
        }
        if let Some(existing) = self
            .completion_blocks
            .iter_mut()
            .find(|context| context.parent == parent && context.span == span)
        {
            for allowed in allowed {
                if !existing.allowed.contains(allowed) {
                    existing.allowed.push(allowed.clone());
                }
            }
            return;
        }
        self.completion_blocks.push(CompletionBlockContext {
            parent: parent.to_owned(),
            allowed: allowed.to_vec(),
            span,
        });
    }

    fn candidate_ids<'a>(
        allowed: &'a [String],
        token: &TokenKind,
        index: Option<&'a BlockCandidateIndex>,
    ) -> Vec<&'a str> {
        index.map_or_else(
            || allowed.iter().map(String::as_str).collect(),
            |index| index.candidates(token).collect(),
        )
    }

    fn consider(&self, best: &mut Option<Candidate>, tied: &mut bool, candidate: Candidate) {
        match best {
            Some(previous) if previous.end_position > candidate.end_position => {}
            Some(previous) if previous.end_position == candidate.end_position => *tied = true,
            _ => {
                *best = Some(candidate);
                *tied = false;
            }
        }
    }

    fn validate_custom_match(
        &mut self,
        construct: &ConstructSpec,
        parsed: CustomConstructMatch,
        _start_position: usize,
    ) -> Result<Candidate, Diagnostic> {
        let parser_id = construct.custom_parser.as_deref().unwrap_or("<unknown>");
        let start = self.current().span.start;
        let Some(end_position) = self.custom_end_position(parsed.span) else {
            return Err(invalid_custom_match(self.current().span, parser_id));
        };
        if parsed.syntax_variant.is_empty()
            || parsed.span.start != start
            || !is_line_end(&self.tokens, end_position)
        {
            return Err(invalid_custom_match(self.current().span, parser_id));
        }
        let mut fields = parsed.fields;
        let field_names = fields.keys().cloned().collect::<Vec<_>>();
        for name in field_names {
            let value = fields.get(&name).expect("field name came from the map");
            let Some(spec) = construct.fields.iter().find(|field| field.name == *name) else {
                return Err(invalid_custom_match(self.current().span, parser_id));
            };
            if value.span.start < parsed.span.start
                || value.span.end > parsed.span.end
                || !valid_span(self.source, value.span)
            {
                return Err(invalid_custom_match(self.current().span, parser_id));
            }
            let mut normalized = value.value.clone();
            if !normalize_custom_value(&mut normalized, &spec.kind, value.span.start)
                || !value_matches(&normalized, &spec.kind)
                || !custom_value_spans_valid(self.source, &normalized, value.span)
            {
                return Err(invalid_custom_match(self.current().span, parser_id));
            }
            fields.get_mut(&name).expect("field was just visited").value = normalized;
        }
        for spec in &construct.fields {
            if !fields.contains_key(&spec.name) {
                if let Some(default) = &spec.default {
                    let Some(value) = default_value(&spec.kind, default) else {
                        return Err(invalid_custom_match(self.current().span, parser_id));
                    };
                    fields.insert(
                        spec.name.clone(),
                        CapturedValue::new(value, Span::new(start, start)),
                    );
                } else if spec.required {
                    return Err(invalid_custom_match(self.current().span, parser_id));
                }
            }
        }
        if parsed
            .diagnostics
            .iter()
            .any(|diagnostic| !valid_diagnostic(self.source, diagnostic))
        {
            return Err(invalid_custom_match(self.current().span, parser_id));
        }
        let custom_children = if let Some(children) = parsed.children {
            Some(self.validate_custom_children(construct, parsed.span, children)?)
        } else {
            None
        };
        let span = parsed.span;
        Ok(Candidate {
            construct_index: self.language.compiled[&construct.id].construct_index,
            fields,
            end_position,
            syntax_variant: parsed.syntax_variant,
            span,
            custom_children,
            diagnostics: parsed.diagnostics,
            host_data: parsed.host_data,
            custom: true,
        })
    }

    fn validate_custom_children(
        &mut self,
        parent: &ConstructSpec,
        parent_span: Span,
        children: Vec<AstNode>,
    ) -> Result<Vec<AstNode>, Diagnostic> {
        let Some(block) = &parent.block else {
            return if children.is_empty() {
                Ok(children)
            } else {
                Err(invalid_custom_match(parent_span, "<custom>"))
            };
        };
        let allowed = self.expanded_allowed(&block.allowed);
        let mut remaining_nodes = self
            .limits
            .max_ast_nodes
            .saturating_sub(self.ast_node_count.saturating_add(1));
        if children.len() > remaining_nodes {
            return Err(Diagnostic::error(
                "LM1302",
                format!(
                    "custom parser AST exceeds the limit of {} nodes",
                    self.limits.max_ast_nodes
                ),
                parent_span,
            ));
        }
        if children
            .windows(2)
            .any(|pair| pair[0].span.end > pair[1].span.start)
        {
            return Err(invalid_custom_match(parent_span, "<custom>"));
        }
        let mut result = Vec::with_capacity(children.len());
        for child in children {
            result.push(self.validate_custom_node(
                child,
                &allowed,
                parent_span,
                1,
                &mut remaining_nodes,
            )?);
        }
        Ok(result)
    }

    fn validate_custom_node(
        &mut self,
        mut node: AstNode,
        allowed: &[String],
        parent_span: Span,
        depth: usize,
        remaining_nodes: &mut usize,
    ) -> Result<AstNode, Diagnostic> {
        if *remaining_nodes == 0 {
            return Err(Diagnostic::error(
                "LM1302",
                format!(
                    "custom parser AST exceeds the limit of {} nodes",
                    self.limits.max_ast_nodes
                ),
                node.span,
            ));
        }
        *remaining_nodes -= 1;
        if depth > self.max_nesting_depth() {
            return Err(Diagnostic::error(
                "LM1302",
                format!(
                    "custom parser AST nesting exceeds the limit of {}",
                    self.max_nesting_depth()
                ),
                node.span,
            ));
        }
        if !allowed.contains(&node.construct)
            || node.syntax_variant.is_empty()
            || node.span.start < parent_span.start
            || node.span.end > parent_span.end
            || node.span.start >= node.span.end
            || !valid_span(self.source, node.span)
        {
            return Err(invalid_custom_match(parent_span, "<custom>"));
        }
        let construct = self
            .construct(&node.construct)
            .ok_or_else(|| invalid_custom_match(node.span, "<custom>"))?;
        if node
            .fields
            .keys()
            .any(|name| !construct.fields.iter().any(|field| field.name == *name))
        {
            return Err(invalid_custom_match(node.span, "<custom>"));
        }
        for spec in &construct.fields {
            match node.fields.get_mut(&spec.name) {
                Some(captured) => {
                    if captured.span.start < node.span.start
                        || captured.span.end > node.span.end
                        || !valid_span(self.source, captured.span)
                        || !normalize_custom_value(
                            &mut captured.value,
                            &spec.kind,
                            captured.span.start,
                        )
                        || !value_matches(&captured.value, &spec.kind)
                        || !custom_value_spans_valid(self.source, &captured.value, captured.span)
                    {
                        return Err(invalid_custom_match(node.span, "<custom>"));
                    }
                }
                None if spec.required && spec.default.is_none() => {
                    return Err(invalid_custom_match(node.span, "<custom>"));
                }
                None => {
                    if let Some(default) = &spec.default {
                        let value = default_value(&spec.kind, default)
                            .ok_or_else(|| invalid_custom_match(node.span, "<custom>"))?;
                        node.fields.insert(
                            spec.name.clone(),
                            CapturedValue::new(value, Span::new(node.span.start, node.span.start)),
                        );
                    }
                }
            }
        }
        if !node.children.is_empty() {
            let block = construct
                .block
                .as_ref()
                .ok_or_else(|| invalid_custom_match(node.span, "<custom>"))?;
            if node
                .children
                .windows(2)
                .any(|pair| pair[0].span.end > pair[1].span.start)
            {
                return Err(invalid_custom_match(node.span, "<custom>"));
            }
            let child_allowed = self.expanded_allowed(&block.allowed);
            if node.children.len() > *remaining_nodes {
                return Err(Diagnostic::error(
                    "LM1302",
                    format!(
                        "custom parser AST exceeds the limit of {} nodes",
                        self.limits.max_ast_nodes
                    ),
                    node.span,
                ));
            }
            let mut normalized = Vec::with_capacity(node.children.len());
            for child in node.children {
                normalized.push(self.validate_custom_node(
                    child,
                    &child_allowed,
                    node.span,
                    depth + 1,
                    remaining_nodes,
                )?);
            }
            node.children = normalized;
        }
        Ok(node)
    }

    fn parse_children(
        &mut self,
        parent_id: &str,
        declared_allowed: &[String],
        open_end: usize,
    ) -> (Vec<AstNode>, Option<usize>) {
        let mut children: Vec<AstNode> = Vec::new();
        let allowed = self.expanded_allowed(declared_allowed);
        let candidate_index = self.language.block_candidates.get(parent_id).cloned();
        let Some(block_layout) = self.language.blocks.get(parent_id).cloned() else {
            self.diagnostics.push(Diagnostic::error(
                "LM1207",
                format!("block `{parent_id}` has no compiled layout"),
                self.current().span,
            ));
            return (children, None);
        };
        let mut close = None;
        let mut has_child = false;
        loop {
            let separator_found = self.skip_block_separators(&block_layout);
            let gap_start = children.last().map_or(open_end, |child| child.span.end);
            let token = self.current();
            let gap_end = match token.kind {
                TokenKind::End => self.source.text.len(),
                _ if self
                    .matches_sequence(self.position, &block_layout.close)
                    .is_some() =>
                {
                    token.span.start
                }
                _ => token.span.end,
            };
            if gap_start <= gap_end {
                self.record_completion_block(
                    parent_id,
                    declared_allowed,
                    Span::new(gap_start, gap_end),
                );
            }
            if let Some(after_close) = self.matches_sequence(self.position, &block_layout.close) {
                if let (Some(first), Some(last)) = (
                    self.tokens.get(self.position),
                    self.tokens.get(after_close.saturating_sub(1)),
                ) {
                    self.block_delimiters.push(BlockDelimiterSpan {
                        construct: parent_id.to_owned(),
                        span: Span::new(first.span.start, last.span.end),
                        opens: false,
                    });
                }
                close = self.tokens.get(after_close - 1).map(|token| token.span.end);
                self.position = after_close;
                break;
            }
            if matches!(self.current().kind, TokenKind::End) {
                self.diagnostics.push(Diagnostic::error(
                    "LM1204",
                    missing_block_close_message(&block_layout),
                    self.current().span,
                ));
                break;
            }
            if has_child
                && block_layout.separator_policy == BlockSeparatorPolicy::Required
                && !separator_found
            {
                self.diagnostics.push(Diagnostic::error(
                    "LM1206",
                    "expected a block separator between child constructs",
                    self.current().span,
                ));
            }
            let start_position = self.position;
            if let Some(node) = self.parse_construct(&allowed, false, candidate_index.as_deref()) {
                children.push(node);
                has_child = true;
            }
            if self.position == start_position {
                self.position += 1;
            }
            if self.resource_limited {
                break;
            }
        }
        (children, close)
    }

    fn expanded_allowed(&self, declared: &[String]) -> Vec<String> {
        let spec = self.language.spec.spec();
        let mut expanded = Vec::new();
        for entry in declared {
            if spec
                .constructs
                .iter()
                .any(|construct| construct.id == *entry)
            {
                if !expanded.contains(entry) {
                    expanded.push(entry.clone());
                }
            } else {
                for construct in &spec.constructs {
                    if construct
                        .implements
                        .iter()
                        .any(|category| category == entry)
                        && !expanded.contains(&construct.id)
                    {
                        expanded.push(construct.id.clone());
                    }
                }
            }
        }
        expanded
    }

    fn construct(&self, id: &str) -> Option<&ConstructSpec> {
        let index = self.language.compiled.get(id)?.construct_index;
        self.language.spec.spec().constructs.get(index)
    }

    fn validate_constraints(
        &mut self,
        construct: &ConstructSpec,
        fields: &BTreeMap<String, CapturedValue>,
    ) {
        for spec in &construct.fields {
            if let Some(captured) = fields.get(&spec.name) {
                self.validate_constraint(spec, captured);
            }
        }
    }

    fn validate_constraint(&mut self, spec: &FieldSpec, value: &CapturedValue) {
        let limits = &spec.constraints;
        match (&spec.kind, &value.value) {
            (FieldKind::Number, Value::Number(number)) => {
                if limits.min.is_some_and(|min| *number < min as f64) {
                    self.diagnostics.push(Diagnostic::error(
                        "LM1230",
                        format!(
                            "field `{}` must be at least {}",
                            spec.name,
                            limits.min.unwrap_or_default()
                        ),
                        value.span,
                    ));
                }
                if limits.max.is_some_and(|max| *number > max as f64) {
                    self.diagnostics.push(Diagnostic::error(
                        "LM1231",
                        format!(
                            "field `{}` must be at most {}",
                            spec.name,
                            limits.max.unwrap_or_default()
                        ),
                        value.span,
                    ));
                }
            }
            (FieldKind::Text, Value::Text(text)) => {
                let count = text.chars().count();
                if limits.min_length.is_some_and(|min| count < min) {
                    self.diagnostics.push(Diagnostic::error(
                        "LM1232",
                        format!("field `{}` is too short", spec.name),
                        value.span,
                    ));
                }
                if limits.max_length.is_some_and(|max| count > max) {
                    self.diagnostics.push(Diagnostic::error(
                        "LM1233",
                        format!("field `{}` is too long", spec.name),
                        value.span,
                    ));
                }
            }
            (FieldKind::List { .. }, Value::List(values)) => {
                self.check_count(spec, values.len(), value.span)
            }
            (FieldKind::Map { .. }, Value::Map(values)) => {
                self.check_count(spec, values.len(), value.span)
            }
            (FieldKind::Properties { .. }, Value::Properties(values)) => {
                self.check_count(spec, values.len(), value.span)
            }
            _ => {}
        }
        if let (FieldKind::Properties { fields, .. }, Value::Properties(values)) =
            (&spec.kind, &value.value)
        {
            for property in fields {
                if let Some(captured) = values.get(&property.name) {
                    self.validate_constraint(property, captured);
                }
            }
        }
    }

    fn check_count(&mut self, spec: &FieldSpec, count: usize, span: Span) {
        if spec
            .constraints
            .min_items
            .is_some_and(|minimum| count < minimum)
        {
            self.diagnostics.push(Diagnostic::error(
                "LM1234",
                format!("field `{}` has too few items", spec.name),
                span,
            ));
        }
        if spec
            .constraints
            .max_items
            .is_some_and(|maximum| count > maximum)
        {
            self.diagnostics.push(Diagnostic::error(
                "LM1235",
                format!("field `{}` has too many items", spec.name),
                span,
            ));
        }
    }

    fn count_node(&mut self, span: Span) -> bool {
        if self.ast_node_count >= self.limits.max_ast_nodes {
            if !self.resource_limited {
                self.diagnostics.push(Diagnostic::error(
                    "LM1302",
                    format!(
                        "AST node count exceeds the limit of {}",
                        self.limits.max_ast_nodes
                    ),
                    span,
                ));
            }
            self.resource_limited = true;
            return false;
        }
        self.ast_node_count += 1;
        true
    }

    fn max_nesting_depth(&self) -> usize {
        self.limits.max_nesting_depth.min(crate::MAX_NESTING_DEPTH)
    }

    fn retain_custom_tree(&mut self, mut node: AstNode) -> Option<AstNode> {
        if !self.count_node(node.span) {
            return None;
        }
        let mut children = Vec::new();
        for child in node.children {
            let Some(child) = self.retain_custom_tree(child) else {
                break;
            };
            children.push(child);
        }
        node.children = children;
        Some(node)
    }

    fn validate_custom_tree_constraints(&mut self, node: &AstNode) {
        if let Some(construct) = self.construct(&node.construct).cloned() {
            self.validate_constraints(&construct, &node.fields);
        }
        for child in &node.children {
            self.validate_custom_tree_constraints(child);
        }
    }

    fn custom_end_position(&self, span: Span) -> Option<usize> {
        if span.start != self.current().span.start
            || span.start >= span.end
            || !valid_span(self.source, span)
        {
            return None;
        }
        self.tokens
            .iter()
            .enumerate()
            .skip(self.position)
            .find(|(_, token)| token.span.end == span.end)
            .map(|(index, _)| index + 1)
    }

    fn skip_balanced_block(&mut self, construct_id: &str) -> Option<usize> {
        let block = self.language.blocks.get(construct_id)?.clone();
        let mut closers = vec![block.close.clone()];
        let mut openers = self.language.blocks.values().collect::<Vec<_>>();
        openers.sort_by(|left, right| {
            right
                .open
                .len()
                .cmp(&left.open.len())
                .then_with(|| left.close_text.cmp(&right.close_text))
        });
        while !matches!(self.current().kind, TokenKind::End) {
            let active_closer = closers.last()?.clone();
            if let Some(after_close) = self.matches_sequence(self.position, &active_closer) {
                closers.pop();
                self.position = after_close;
                let end = self
                    .tokens
                    .get(self.position.saturating_sub(1))
                    .map(|token| token.span.end)?;
                if closers.is_empty() {
                    return Some(end);
                }
                continue;
            }
            if let Some((nested, after_open)) = openers.iter().find_map(|candidate| {
                self.matches_sequence(self.position, &candidate.open)
                    .map(|after| (*candidate, after))
            }) {
                closers.push(nested.close.clone());
                self.position = after_open;
                continue;
            }
            self.position += 1;
        }
        self.diagnostics.push(Diagnostic::error(
            "LM1204",
            missing_block_close_message(&block),
            self.current().span,
        ));
        None
    }

    fn skip_block_separators(&mut self, block: &CompiledBlock) -> bool {
        let mut found = false;
        loop {
            match self.current().kind {
                TokenKind::Comment => self.position += 1,
                TokenKind::Newline => {
                    found |= block
                        .separators
                        .iter()
                        .any(|separator| matches!(separator, CompiledBlockSeparator::Newline));
                    self.position += 1;
                }
                _ => {
                    let matched = block
                        .separators
                        .iter()
                        .find_map(|separator| match separator {
                            CompiledBlockSeparator::Newline => None,
                            CompiledBlockSeparator::Literal(literal) => {
                                self.matches_sequence(self.position, literal)
                            }
                        });
                    let Some(after) = matched else {
                        break;
                    };
                    found = true;
                    self.position = after;
                }
            }
        }
        found
    }

    fn matches_sequence(&self, start: usize, expected: &[TokenKind]) -> Option<usize> {
        if expected.is_empty() {
            return None;
        }
        let mut position = start;
        for kind in expected {
            while self
                .tokens
                .get(position)
                .is_some_and(|token| matches!(token.kind, TokenKind::Newline | TokenKind::Comment))
            {
                position += 1;
            }
            if self.tokens.get(position)?.kind != *kind {
                return None;
            }
            position += 1;
        }
        Some(position)
    }

    fn sequence_start_ending_at(&self, end: usize, expected: &[TokenKind]) -> Option<usize> {
        if expected.is_empty() || end > self.tokens.len() {
            return None;
        }
        let mut position = end;
        for expected_kind in expected.iter().rev() {
            while position > 0
                && matches!(
                    self.tokens[position - 1].kind,
                    TokenKind::Newline | TokenKind::Comment
                )
            {
                position -= 1;
            }
            position = position.checked_sub(1)?;
            if self.tokens.get(position)?.kind != *expected_kind {
                return None;
            }
        }
        Some(position)
    }

    fn range(&self, start: usize, end: usize) -> Span {
        let start = self
            .tokens
            .get(start)
            .map_or(self.source.text.len(), |token| token.span.start);
        let finish = end
            .checked_sub(1)
            .and_then(|index| self.tokens.get(index))
            .map_or(start, |token| token.span.end);
        Span::new(start, finish)
    }

    fn current(&self) -> &LanguageToken {
        &self.tokens[self.position.min(self.tokens.len() - 1)]
    }
    fn current_line_span(&self) -> Span {
        let start = self.current().span.start;
        let end = self.tokens[self.position..]
            .iter()
            .take_while(|token| {
                !matches!(
                    token.kind,
                    TokenKind::Newline
                        | TokenKind::Comment
                        | TokenKind::End
                        | TokenKind::Punctuation('}')
                )
            })
            .last()
            .map_or(start, |token| token.span.end);
        Span::new(start, end)
    }
    fn skip_separators(&mut self) {
        while matches!(
            self.current().kind,
            TokenKind::Newline | TokenKind::Comment | TokenKind::Punctuation(';')
        ) {
            self.position += 1;
        }
    }
    fn recover_line(&mut self) {
        while !matches!(
            self.current().kind,
            TokenKind::Newline | TokenKind::Comment | TokenKind::End | TokenKind::Punctuation('}')
        ) {
            self.position += 1;
        }
        if matches!(self.current().kind, TokenKind::Newline | TokenKind::Comment) {
            self.position += 1;
        }
    }
}

fn merge_completion_frontier(
    current: &mut Option<(usize, CompletionFrontier)>,
    construct: &str,
    syntax_variant: &str,
    progress: Option<PatternProgress>,
) {
    let Some(progress) = progress else {
        return;
    };
    let mut captures = progress
        .captures
        .into_iter()
        .map(|capture| ExpectedCapture {
            construct: construct.to_owned(),
            syntax_variant: syntax_variant.to_owned(),
            field: capture.field,
            kind: capture.kind,
        })
        .collect::<Vec<_>>();
    let overflowed = captures.len() > MAX_COMPLETION_EXPECTED_CAPTURES;
    captures.truncate(MAX_COMPLETION_EXPECTED_CAPTURES);
    let mut candidate = CompletionFrontier {
        span: progress.span,
        ambiguous: progress.has_non_capture || overflowed || captures.len() > 1,
        captures,
        continuation: progress.continuation,
    };
    let Some((current_position, current_frontier)) = current else {
        *current = Some((progress.token_position, candidate));
        return;
    };
    if progress.token_position > *current_position {
        *current = Some((progress.token_position, candidate));
        return;
    }
    if progress.token_position < *current_position {
        return;
    }
    if current_frontier.span != candidate.span {
        current_frontier.ambiguous = true;
        current_frontier.continuation = None;
        return;
    }
    if current_frontier.continuation != candidate.continuation {
        current_frontier.continuation = None;
    }
    if current_frontier.ambiguous || candidate.ambiguous {
        current_frontier.ambiguous = true;
        return;
    }
    for capture in &candidate.captures {
        if !current_frontier.captures.iter().any(|existing| {
            existing.construct == capture.construct
                && existing.field == capture.field
                && existing.kind == capture.kind
        }) {
            current_frontier.ambiguous = true;
            return;
        }
    }
    current_frontier.captures.append(&mut candidate.captures);
}

fn apply_defaults(
    construct: &ConstructSpec,
    fields: &mut BTreeMap<String, CapturedValue>,
    offset: usize,
    errors: &mut Option<Diagnostic>,
) {
    for spec in &construct.fields {
        if fields.contains_key(&spec.name) {
            continue;
        }
        if let Some(default) = &spec.default {
            if let Some(value) = default_value(&spec.kind, default) {
                fields.insert(
                    spec.name.clone(),
                    CapturedValue::new(value, Span::new(offset, offset)),
                );
            } else if errors.is_none() {
                *errors = Some(Diagnostic::error(
                    "LM1207",
                    format!(
                        "invalid configured default for field `{}.{}`",
                        construct.id, spec.name
                    ),
                    Span::new(offset, offset),
                ));
            }
        }
    }
}

fn normalize_custom_value(value: &mut Value, kind: &FieldKind, default_offset: usize) -> bool {
    match (&mut *value, kind) {
        (Value::Properties(values), FieldKind::Properties { fields, .. }) => {
            if values
                .keys()
                .any(|name| !fields.iter().any(|field| field.name == *name))
            {
                return false;
            }
            for spec in fields {
                if let Some(captured) = values.get_mut(&spec.name) {
                    if !normalize_custom_value(&mut captured.value, &spec.kind, captured.span.start)
                    {
                        return false;
                    }
                } else if let Some(default) = &spec.default {
                    let Some(value) = default_value(&spec.kind, default) else {
                        return false;
                    };
                    values.insert(
                        spec.name.clone(),
                        CapturedValue::new(value, Span::new(default_offset, default_offset)),
                    );
                } else if spec.required {
                    return false;
                }
            }
            true
        }
        (Value::List(values), FieldKind::List { item }) => values
            .iter_mut()
            .all(|value| normalize_custom_value(&mut value.value, item, value.span.start)),
        (Value::Map(entries), FieldKind::Map { value, .. }) => entries.iter_mut().all(|entry| {
            normalize_custom_value(&mut entry.value.value, value, entry.value.span.start)
        }),
        _ => value_matches(value, kind),
    }
}

fn value_matches(value: &Value, kind: &FieldKind) -> bool {
    match (value, kind) {
        (Value::Identifier(_), FieldKind::Identifier)
        | (Value::Text(_), FieldKind::Text)
        | (Value::Reference(_), FieldKind::Reference { .. }) => true,
        (Value::Number(value), FieldKind::Number) => value.is_finite(),
        (Value::Enum(value), FieldKind::Enum { values } | FieldKind::NamedEnum { values, .. }) => {
            values.contains(value)
        }
        (Value::Properties(values), FieldKind::Properties { fields, .. }) => {
            fields.iter().all(|field| {
                values
                    .get(&field.name)
                    .is_some_and(|value| value_matches(&value.value, &field.kind))
                    || (!field.required && field.default.is_none())
            }) && values
                .keys()
                .all(|name| fields.iter().any(|field| field.name == *name))
        }
        (Value::List(values), FieldKind::List { item }) => {
            values.iter().all(|value| value_matches(&value.value, item))
        }
        (Value::Map(entries), FieldKind::Map { key, value }) => {
            let mut seen = BTreeSet::new();
            entries.iter().all(|entry| {
                let key_kind_matches = matches!(
                    (entry.key_type, key),
                    (ValueType::Identifier, MapKeyKind::Identifier)
                        | (ValueType::Text, MapKeyKind::Text)
                ) && (matches!(key, MapKeyKind::Text)
                    || valid_map_identifier(&entry.key));
                key_kind_matches
                    && seen.insert(entry.key.as_str())
                    && value_matches(&entry.value.value, value)
            })
        }
        (Value::Expression(_), FieldKind::Expression { .. }) => true,
        _ => false,
    }
}

fn custom_value_spans_valid(source: &SourceFile, value: &Value, envelope: Span) -> bool {
    let contained = |span: Span| {
        valid_span(source, span) && span.start >= envelope.start && span.end <= envelope.end
    };
    match value {
        Value::Properties(fields) => fields.values().all(|captured| {
            contained(captured.span)
                && custom_value_spans_valid(source, &captured.value, captured.span)
        }),
        Value::List(items) => {
            let mut previous_end = envelope.start;
            items.iter().all(|captured| {
                let ordered = captured.span.start >= previous_end && contained(captured.span);
                previous_end = captured.span.end;
                ordered && custom_value_spans_valid(source, &captured.value, captured.span)
            })
        }
        Value::Map(entries) => {
            let mut previous_end = envelope.start;
            entries.iter().all(|entry| {
                let valid_entry = contained(entry.key_span)
                    && contained(entry.value.span)
                    && entry.key_span.start >= previous_end
                    && entry.key_span.end <= entry.value.span.start
                    && custom_value_spans_valid(source, &entry.value.value, entry.value.span);
                previous_end = entry.value.span.end;
                valid_entry
            })
        }
        Value::Expression(expression) => expression_spans_valid(expression, &contained),
        Value::Identifier(_)
        | Value::Number(_)
        | Value::Text(_)
        | Value::Enum(_)
        | Value::Reference(_) => true,
    }
}

fn expression_spans_valid(
    expression: &langmeld_expression::Expression,
    contained: &impl Fn(Span) -> bool,
) -> bool {
    use langmeld_expression::Expression;

    match expression {
        Expression::Atom(_, span) => contained(*span),
        Expression::Prefix {
            operator_span,
            operand,
            span,
            ..
        } => {
            contained(*span)
                && contained(*operator_span)
                && expression_spans_valid(operand, contained)
        }
        Expression::Infix {
            left,
            operator_span,
            right,
            span,
            ..
        } => {
            contained(*span)
                && contained(*operator_span)
                && expression_spans_valid(left, contained)
                && expression_spans_valid(right, contained)
        }
        Expression::Group {
            expression, span, ..
        } => contained(*span) && expression_spans_valid(expression, contained),
        Expression::Call {
            callee,
            arguments,
            span,
        } => {
            contained(*span)
                && expression_spans_valid(callee, contained)
                && arguments
                    .iter()
                    .all(|argument| expression_spans_valid(argument, contained))
        }
        Expression::Member {
            receiver,
            name_span,
            span,
            ..
        } => {
            contained(*span) && contained(*name_span) && expression_spans_valid(receiver, contained)
        }
        Expression::Index {
            receiver,
            index,
            span,
        } => {
            contained(*span)
                && expression_spans_valid(receiver, contained)
                && expression_spans_valid(index, contained)
        }
    }
}

fn valid_map_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|character| character == '_' || character.is_alphabetic())
        && chars
            .all(|character| character == '_' || character == '-' || character.is_alphanumeric())
}

fn valid_span(source: &SourceFile, span: Span) -> bool {
    span.start <= span.end
        && span.end <= source.text.len()
        && source.text.is_char_boundary(span.start)
        && source.text.is_char_boundary(span.end)
}

fn missing_block_close_message(block: &CompiledBlock) -> String {
    if block.close_text == "}" {
        "expected `}` before the end of the file".to_owned()
    } else {
        format!(
            "expected block closer `{}` before the end of the file",
            block.close_text
        )
    }
}

fn valid_diagnostic(source: &SourceFile, diagnostic: &Diagnostic) -> bool {
    diagnostic.span.is_none_or(|span| valid_span(source, span))
}
fn is_line_end(tokens: &[LanguageToken], position: usize) -> bool {
    matches!(
        tokens.get(position).map(|token| &token.kind),
        Some(
            TokenKind::Newline
                | TokenKind::Comment
                | TokenKind::End
                | TokenKind::Punctuation('}' | ';')
        ) | None
    )
}
fn is_punct(kind: &TokenKind, expected: char) -> bool {
    matches!(kind, TokenKind::Punctuation(value) if *value == expected)
}
fn invalid_custom_match(span: Span, parser_id: &str) -> Diagnostic {
    Diagnostic::error(
        "LM1209",
        format!("custom parser `{parser_id}` returned an invalid construct match"),
        span,
    )
}
