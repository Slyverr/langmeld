//! Candidate selection, AST construction, and recoverable block parsing.

use std::collections::{BTreeMap, BTreeSet};

use langmeld_core::{
    ConstructSpec, Diagnostic, FieldKind, FieldSpec, MapKeyKind, SourceFile, Span,
};

use crate::{
    AstNode, CapturedValue, CustomConstructMatch, CustomConstructParser, LanguageToken,
    LanguageTokenKind as TokenKind, ParseLimits, Program, Value, ValueType,
};

use super::{
    candidates::BlockCandidateIndex,
    matching::{PatternMatcher, default_value},
    prepared::{ConfiguredLanguageParser, PreparedLanguage},
};

pub(super) fn parse_tokens(
    parser: &ConfiguredLanguageParser,
    source: &SourceFile,
    tokens: Vec<LanguageToken>,
    limits: ParseLimits,
) -> (Program, Vec<Diagnostic>, Vec<Span>, bool) {
    let (program, diagnostics, custom_parser_spans, resource_limited) = Parser {
        language: &parser.language,
        source,
        tokens,
        custom_parsers: &parser.parsers,
        position: 0,
        diagnostics: Vec::new(),
        custom_parser_spans: Vec::new(),
        depth: 0,
        ast_node_count: 0,
        limits,
        remaining_pattern_steps: limits.max_pattern_steps,
        resource_limited: false,
    }
    .parse();
    (program, diagnostics, custom_parser_spans, resource_limited)
}

struct Parser<'a> {
    language: &'a PreparedLanguage,
    source: &'a SourceFile,
    tokens: Vec<LanguageToken>,
    custom_parsers: &'a [CustomConstructParser],
    position: usize,
    diagnostics: Vec<Diagnostic>,
    custom_parser_spans: Vec<Span>,
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
    fn parse(mut self) -> (Program, Vec<Diagnostic>, Vec<Span>, bool) {
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
        (
            Program { nodes },
            self.diagnostics,
            self.custom_parser_spans,
            self.resource_limited,
        )
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
                );
                let matched = matcher.match_pattern(&pattern.parts, start_position);
                let matched = match matched {
                    Ok(value) => value,
                    Err(diagnostic) if diagnostic.code == "LM1304" => {
                        self.resource_limited = true;
                        self.diagnostics.push(diagnostic);
                        self.recover_line();
                        return None;
                    }
                    Err(diagnostic) => {
                        if best_value_error.is_none() {
                            best_value_error = Some(diagnostic);
                        }
                        continue;
                    }
                };
                let Some(mut matched) = matched else { continue };
                if let Err(diagnostic) = matched.normalize_record_captures(construct) {
                    if best_value_error.is_none() {
                        best_value_error = Some(diagnostic);
                    }
                    continue;
                }
                let block_opener = construct.block.is_some()
                    && matched.end_position > start_position
                    && is_punct(&self.tokens[matched.end_position - 1].kind, '{');
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
            && candidate.end_position > start_position
            && is_punct(&self.tokens[candidate.end_position - 1].kind, '{');
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
                if let Some(close) = self.skip_balanced_block() {
                    node.span.end = close;
                }
            } else {
                self.depth += 1;
                let children = self.parse_children(&construct.id, &block.allowed);
                self.depth -= 1;
                node.children = children.0;
                if let Some(close) = children.1 {
                    node.span.end = close;
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
    ) -> (Vec<AstNode>, Option<usize>) {
        let mut children = Vec::new();
        let allowed = self.expanded_allowed(declared_allowed);
        let candidate_index = self.language.block_candidates.get(parent_id).cloned();
        let mut close = None;
        loop {
            self.skip_separators();
            match self.current().kind {
                TokenKind::Punctuation('}') => {
                    close = Some(self.current().span.end);
                    self.position += 1;
                    break;
                }
                TokenKind::End => {
                    self.diagnostics.push(Diagnostic::error(
                        "LM1204",
                        "expected `}` before the end of the file",
                        self.current().span,
                    ));
                    break;
                }
                _ => {
                    if let Some(node) =
                        self.parse_construct(&allowed, false, candidate_index.as_deref())
                    {
                        children.push(node);
                    }
                    if self.resource_limited {
                        break;
                    }
                }
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

    fn skip_balanced_block(&mut self) -> Option<usize> {
        let mut nesting = 1_usize;
        while !matches!(self.current().kind, TokenKind::End) {
            match self.current().kind {
                TokenKind::Punctuation('{') => nesting += 1,
                TokenKind::Punctuation('}') => {
                    nesting -= 1;
                    let end = self.current().span.end;
                    self.position += 1;
                    if nesting == 0 {
                        return Some(end);
                    }
                    continue;
                }
                _ => {}
            }
            self.position += 1;
        }
        self.diagnostics.push(Diagnostic::error(
            "LM1204",
            "expected `}` before the end of the file",
            self.current().span,
        ));
        None
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
        Value::Identifier(_)
        | Value::Number(_)
        | Value::Text(_)
        | Value::Enum(_)
        | Value::Reference(_) => true,
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
