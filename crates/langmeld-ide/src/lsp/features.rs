//! What the server answers about one document, worked out from its tree.

use std::collections::HashMap;

use langmeld_meta::{Editor, EditorRule};
use langmeld_syntax::{
    Language, Parse, SourceFile, SyntaxElement, SyntaxNode, SyntaxToken, TextRange, TextSize,
    TokenClass,
};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, DocumentSymbol, FoldingRange,
    FoldingRangeKind, SelectionRange, SemanticToken, SemanticTokenModifier, SemanticTokenType,
    SymbolKind, TextEdit,
};

use super::position::{Encoding, position, range};

/// One document and the language it is written in.
pub(crate) struct View<'a> {
    pub(crate) language: &'a Language,
    pub(crate) editor: &'a Editor,
    pub(crate) file: &'a SourceFile,
    pub(crate) parse: &'a Parse,
    pub(crate) encoding: Encoding,
}

/// The kinds of token the server colours, in the order of its legend.
pub(crate) const TOKEN_TYPES: [SemanticTokenType; 10] = [
    SemanticTokenType::KEYWORD,
    SemanticTokenType::COMMENT,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::CLASS,
    SemanticTokenType::NAMESPACE,
    SemanticTokenType::PROPERTY,
];

pub(crate) const TOKEN_MODIFIERS: [SemanticTokenModifier; 2] = [
    SemanticTokenModifier::DOCUMENTATION,
    SemanticTokenModifier::DECLARATION,
];

const KEYWORD: u32 = 0;
const COMMENT: u32 = 1;
const STRING: u32 = 2;
const NUMBER: u32 = 3;
const OPERATOR: u32 = 4;
const VARIABLE: u32 = 5;
const DOCUMENTATION: u32 = 1;
const DECLARATION: u32 = 2;

/// Punctuation that separates or groups, which is not coloured as an operator.
const QUIET: [&str; 10] = ["(", ")", "{", "}", "[", "]", ";", ",", ".", ":"];

pub(crate) fn symbol_kind(name: &str) -> SymbolKind {
    match name {
        "file" => SymbolKind::FILE,
        "module" => SymbolKind::MODULE,
        "namespace" => SymbolKind::NAMESPACE,
        "package" => SymbolKind::PACKAGE,
        "class" => SymbolKind::CLASS,
        "method" => SymbolKind::METHOD,
        "property" => SymbolKind::PROPERTY,
        "field" => SymbolKind::FIELD,
        "constructor" => SymbolKind::CONSTRUCTOR,
        "enum" => SymbolKind::ENUM,
        "interface" => SymbolKind::INTERFACE,
        "function" => SymbolKind::FUNCTION,
        "constant" => SymbolKind::CONSTANT,
        "string" => SymbolKind::STRING,
        "number" => SymbolKind::NUMBER,
        "boolean" => SymbolKind::BOOLEAN,
        "array" => SymbolKind::ARRAY,
        "object" => SymbolKind::OBJECT,
        "key" => SymbolKind::KEY,
        "enum_member" => SymbolKind::ENUM_MEMBER,
        "struct" => SymbolKind::STRUCT,
        "event" => SymbolKind::EVENT,
        "operator" => SymbolKind::OPERATOR,
        "type" => SymbolKind::TYPE_PARAMETER,
        _ => SymbolKind::VARIABLE,
    }
}

/// The kind of completion for a name whose symbol is of the kind `symbol`.
pub(crate) fn completion_kind(symbol: Option<&str>) -> CompletionItemKind {
    match symbol {
        Some("function") => CompletionItemKind::FUNCTION,
        Some("method") => CompletionItemKind::METHOD,
        Some("constructor") => CompletionItemKind::CONSTRUCTOR,
        Some("constant") => CompletionItemKind::CONSTANT,
        Some("class") => CompletionItemKind::CLASS,
        Some("struct") => CompletionItemKind::STRUCT,
        Some("interface") => CompletionItemKind::INTERFACE,
        Some("enum") => CompletionItemKind::ENUM,
        Some("enum_member") => CompletionItemKind::ENUM_MEMBER,
        Some("module" | "namespace" | "package") => CompletionItemKind::MODULE,
        Some("property") => CompletionItemKind::PROPERTY,
        Some("field" | "key") => CompletionItemKind::FIELD,
        Some("type") => CompletionItemKind::TYPE_PARAMETER,
        _ => CompletionItemKind::VARIABLE,
    }
}

/// The word around `offset`: letters, digits and `_`. Empty where there is none.
pub(crate) fn word_around(text: &str, offset: TextSize) -> TextRange {
    let at = usize::from(offset).min(text.len());
    let (before, after) = text.split_at_checked(at).unwrap_or((text, ""));
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let start = before.trim_end_matches(word).len();
    let end = text.len() - after.trim_start_matches(word).len();
    let size = |at: usize| TextSize::of(text.get(..at).unwrap_or_default());
    TextRange::new(size(start), size(end))
}

pub(crate) fn token_type(symbol: &str) -> u32 {
    match symbol {
        "function" | "method" => 6,
        "class" | "struct" | "interface" | "enum" | "type" => 7,
        "module" | "namespace" | "package" => 8,
        "property" | "field" | "key" => 9,
        _ => VARIABLE,
    }
}

impl View<'_> {
    fn name(&self, node: &SyntaxNode) -> &str {
        self.language.kind_name(node.kind())
    }

    /// The part of an element that is not space or comments, where it has any.
    pub(crate) fn trimmed(&self, element: &SyntaxElement) -> TextRange {
        let SyntaxElement::Node(node) = element else {
            return element.text_range();
        };
        let mut tokens = node
            .descendants_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .filter(|token| !self.language.is_trivia(token.kind()));
        match tokens.next() {
            Some(first) => {
                let last = tokens.last().unwrap_or_else(|| first.clone());
                first.text_range().cover(last.text_range())
            }
            None => TextRange::empty(node.text_range().start()),
        }
    }

    /// The nodes the editor rules list as symbols: each with its name, where that
    /// is, and its kind.
    pub(crate) fn declarations(&self) -> Vec<(SyntaxNode, TextRange, String)> {
        let rules: HashMap<&str, (&str, &str)> = self
            .editor
            .rules
            .iter()
            .filter_map(|rule| match rule {
                EditorRule::Symbol {
                    node, kind, name, ..
                } => Some((node.as_str(), (kind.as_str(), name.as_str()))),
                _ => None,
            })
            .collect();
        if rules.is_empty() {
            return Vec::new();
        }
        self.parse
            .syntax()
            .descendants()
            .filter_map(|node| {
                let &(kind, label) = rules.get(self.name(&node))?;
                let found = self.language.field(&node, label).into_iter().next()?;
                let name = self.trimmed(&found);
                (!name.is_empty()).then(|| (node, name, kind.to_owned()))
            })
            .collect()
    }

    pub(crate) fn text(&self, range: TextRange) -> &str {
        self.file
            .text()
            .get(usize::from(range.start())..usize::from(range.end()))
            .unwrap_or_default()
    }

    pub(crate) fn symbols(&self) -> Vec<DocumentSymbol> {
        // Symbols still open, outermost first, with where their nodes end.
        let mut open: Vec<(TextSize, DocumentSymbol)> = Vec::new();
        let mut done = Vec::new();
        let close = |open: &mut Vec<(TextSize, DocumentSymbol)>, done: &mut Vec<_>| {
            if let Some((_, symbol)) = open.pop() {
                match open.last_mut() {
                    Some((_, parent)) => parent.children.get_or_insert_default().push(symbol),
                    None => done.push(symbol),
                }
            }
        };
        for (node, name, kind) in self.declarations() {
            let node_range = self.trimmed(&SyntaxElement::Node(node));
            while open
                .last()
                .is_some_and(|(end, _)| *end <= node_range.start())
            {
                close(&mut open, &mut done);
            }
            #[allow(deprecated)] // The field must be given.
            let symbol = DocumentSymbol {
                name: self.text(name).to_owned(),
                detail: None,
                kind: symbol_kind(&kind),
                tags: None,
                deprecated: None,
                range: range(self.file, node_range.cover(name), self.encoding),
                selection_range: range(self.file, name, self.encoding),
                children: None,
            };
            open.push((node_range.end(), symbol));
        }
        while !open.is_empty() {
            close(&mut open, &mut done);
        }
        done
    }

    pub(crate) fn folds(&self) -> Vec<FoldingRange> {
        let listed: Vec<&str> = self
            .editor
            .rules
            .iter()
            .filter_map(|rule| match rule {
                EditorRule::Fold { nodes, .. } => Some(nodes),
                _ => None,
            })
            .flatten()
            .map(String::as_str)
            .collect();
        let line = |offset| position(self.file, offset, self.encoding).line;
        let fold = |start_line, end_line, kind| FoldingRange {
            start_line,
            end_line,
            kind,
            ..FoldingRange::default()
        };
        let mut folds = Vec::new();
        for element in self.parse.syntax().descendants_with_tokens() {
            match &element {
                SyntaxElement::Node(node) => {
                    let braced = || {
                        node.children_with_tokens()
                            .find(|child| !self.language.is_trivia(child.kind()))
                            .and_then(SyntaxElement::into_token)
                            .is_some_and(|token| token.text() == "{")
                    };
                    // Without fold rules, whatever stands in braces folds.
                    let folds_away = if listed.is_empty() {
                        braced()
                    } else {
                        listed.contains(&self.name(node))
                    };
                    if !folds_away {
                        continue;
                    }
                    let span = self.trimmed(&element);
                    let (start, mut end) = (line(span.start()), line(span.end()));
                    // A closing bracket stays in sight.
                    if self.text(span).ends_with(['}', ')', ']']) {
                        end = end.saturating_sub(1);
                    }
                    if end > start {
                        folds.push(fold(start, end, None));
                    }
                }
                SyntaxElement::Token(token) => {
                    let comment = matches!(
                        self.language.token_class(token.kind()),
                        Some(TokenClass::Comment | TokenClass::DocComment)
                    );
                    let span = token.text_range();
                    let (start, end) = (line(span.start()), line(span.end()));
                    if comment && end > start {
                        folds.push(fold(start, end, Some(FoldingRangeKind::Comment)));
                    }
                }
            }
        }
        folds.sort_by_key(|fold| (fold.start_line, fold.end_line));
        folds.dedup_by_key(|fold| fold.start_line);
        folds
    }

    /// The token at `offset`, preferring one that rules see to the space beside it.
    fn token_at(&self, offset: TextSize) -> Option<SyntaxToken> {
        let found = self.parse.syntax().token_at_offset(offset);
        let (left, right) = (found.clone().left_biased(), found.right_biased());
        match right {
            Some(token) if !self.language.is_trivia(token.kind()) => Some(token),
            right => left.or(right),
        }
    }

    /// The ranges around `offset`, from its token out to the file.
    pub(crate) fn selection(&self, offset: TextSize) -> SelectionRange {
        let mut ranges = Vec::new();
        if let Some(token) = self.token_at(offset) {
            ranges.push(token.text_range());
            let mut node = token.parent();
            while let Some(current) = node {
                // The root spans the whole file, space and all.
                let span = match current.parent() {
                    Some(_) => self.trimmed(&SyntaxElement::Node(current.clone())),
                    None => current.text_range(),
                };
                if ranges
                    .last()
                    .is_some_and(|last| *last != span && span.contains_range(*last))
                {
                    ranges.push(span);
                }
                node = current.parent();
            }
        }
        let mut found: Option<SelectionRange> = None;
        for span in ranges.into_iter().rev() {
            found = Some(SelectionRange {
                range: range(self.file, span, self.encoding),
                parent: found.map(Box::new),
            });
        }
        found.unwrap_or_else(|| SelectionRange {
            range: range(self.file, TextRange::empty(offset), self.encoding),
            parent: None,
        })
    }

    /// The colours of the tokens, with `uses` giving those of names by what they
    /// mean, by where each starts.
    pub(crate) fn semantic_tokens(&self, uses: &HashMap<TextSize, u32>) -> Vec<SemanticToken> {
        let names: HashMap<TextSize, u32> = self
            .declarations()
            .into_iter()
            .map(|(_, name, kind)| (name.start(), token_type(&kind)))
            .collect();
        let mut data = Vec::new();
        let (mut line, mut column) = (0, 0);
        let tokens = self
            .parse
            .syntax()
            .descendants_with_tokens()
            .filter_map(SyntaxElement::into_token);
        for token in tokens {
            let span = token.text_range();
            let declared = names.get(&span.start()).copied();
            let (kind, modifiers) = match self.language.token_class(token.kind()) {
                Some(TokenClass::Keyword | TokenClass::ContextualKeyword) => (KEYWORD, 0),
                Some(TokenClass::Comment) => (COMMENT, 0),
                Some(TokenClass::DocComment) => (COMMENT, DOCUMENTATION),
                Some(TokenClass::String) => (STRING, 0),
                Some(TokenClass::Number) => (NUMBER, 0),
                Some(TokenClass::Punctuation) if !QUIET.contains(&token.text()) => (OPERATOR, 0),
                Some(TokenClass::Identifier) => match declared {
                    Some(kind) => (kind, DECLARATION),
                    None => (uses.get(&span.start()).copied().unwrap_or(VARIABLE), 0),
                },
                _ => continue,
            };
            // A token of several lines is coloured line by line.
            let mut start = position(self.file, span.start(), self.encoding);
            for piece in token.text().split_inclusive('\n') {
                let shown = piece.trim_end_matches(['\r', '\n']);
                let length = self.encoding.length(shown);
                if length > 0 && start.line >= line {
                    let delta_line = start.line - line;
                    let from = if delta_line == 0 { column } else { 0 };
                    data.push(SemanticToken {
                        delta_line,
                        delta_start: start.character.saturating_sub(from),
                        length,
                        token_type: kind,
                        token_modifiers_bitset: modifiers,
                    });
                    (line, column) = (start.line, start.character);
                }
                start.line = start.line.saturating_add(1);
                start.character = 0;
            }
        }
        data
    }

    /// The text before the word at `offset`, and the range of the word, which a
    /// completion replaces. The word is what is being written, whole.
    fn word_at(&self, offset: TextSize) -> (&str, lsp_types::Range) {
        let text = self.file.text();
        let word = word_around(text, offset);
        let prefix = text.get(..usize::from(word.start())).unwrap_or_default();
        (prefix, range(self.file, word, self.encoding))
    }

    fn items(
        words: impl IntoIterator<Item = String>,
        replaced: lsp_types::Range,
        kind: CompletionItemKind,
    ) -> Vec<CompletionItem> {
        words
            .into_iter()
            .map(|word| CompletionItem {
                label: word.clone(),
                kind: Some(kind),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: replaced,
                    new_text: word,
                })),
                ..CompletionItem::default()
            })
            .collect()
    }

    /// The keywords that may be written at `offset`, replacing the word there.
    pub(crate) fn completions(&self, offset: TextSize) -> Vec<CompletionItem> {
        let (prefix, replaced) = self.word_at(offset);
        let mut words: Vec<String> = self
            .language
            .expected(prefix)
            .into_iter()
            .filter(|&kind| {
                matches!(
                    self.language.token_class(kind),
                    Some(TokenClass::Keyword | TokenClass::ContextualKeyword)
                )
            })
            .map(|kind| self.language.kind_name(kind).to_owned())
            .collect();
        words.sort_unstable();
        words.dedup();
        Self::items(words, replaced, CompletionItemKind::KEYWORD)
    }

    /// Words of the language's own, offered where `offset` is.
    pub(crate) fn words(&self, offset: TextSize, words: Vec<String>) -> Vec<CompletionItem> {
        let (_, replaced) = self.word_at(offset);
        Self::items(words, replaced, CompletionItemKind::TEXT)
    }
}
