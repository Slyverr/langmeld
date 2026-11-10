//! Frontend for the first user-facing `.lm` language-definition syntax.

use std::sync::OnceLock;

use langmeld_core::{Diagnostic, SourceFile, Span};

use crate::{
    CapturedValue, ConstructSpec, FieldKind, LanguageSpec, ParsedConstruct, PatternPart, SpecError,
    parse,
};

/// Result of parsing and validating one language-definition source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionResult {
    /// Validated language when the complete definition has no errors.
    pub language: Option<LanguageSpec>,
    /// Parser and definition diagnostics tied to the source snapshot.
    pub diagnostics: Vec<Diagnostic>,
}

impl DefinitionResult {
    /// Returns whether a valid language was produced.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.language.is_some() && self.diagnostics.is_empty()
    }
}

/// Parses a modular-language source file and validates its declared grammar.
///
/// The bootstrap grammar is authored in Rust and parsed by Langmeld's generic
/// parser. This keeps the definition frontend on the same parser path that
/// user-defined languages use. The accepted subset currently describes
/// keyword-led token patterns and delimited blocks.
#[must_use]
pub fn parse_language_definition(source: &SourceFile) -> DefinitionResult {
    let parsed = parse(source, definition_grammar());
    let mut diagnostics = parsed.diagnostics;
    let language_nodes: Vec<_> = parsed
        .constructs
        .iter()
        .filter(|node| node.name == "language")
        .collect();

    if language_nodes.is_empty() {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-001",
            "expected one `language` declaration",
            source,
            Span::new(0, 0),
        ));
        return DefinitionResult {
            language: None,
            diagnostics,
        };
    }
    if language_nodes.len() > 1 {
        for node in language_nodes.iter().skip(1) {
            diagnostics.push(Diagnostic::error(
                "LM-DEF-002",
                "a definition file must contain exactly one `language` declaration",
                source,
                node.span,
            ));
        }
    }

    let root = language_nodes[0];
    let Some(language_name) = string_field(root, "name").and_then(decode_quoted) else {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-003",
            "language declaration requires a valid quoted name",
            source,
            root.span,
        ));
        return DefinitionResult {
            language: None,
            diagnostics,
        };
    };

    let mut constructs = Vec::new();
    for node in root.children.iter().filter(|node| node.name == "construct") {
        if let Some(construct) = lower_construct(node, source, &mut diagnostics) {
            constructs.push(construct);
        }
    }

    let language = match LanguageSpec::new(language_name, constructs) {
        Ok(language) => Some(language),
        Err(error) => {
            let span = spec_error_construct(&error)
                .and_then(|name| {
                    root.children
                        .iter()
                        .find(|node| {
                            node.name == "construct"
                                && string_field(node, "name")
                                    .and_then(decode_quoted)
                                    .as_deref()
                                    == Some(name)
                        })
                        .map(|node| node.span)
                })
                .unwrap_or(root.span);
            diagnostics.push(Diagnostic::error(
                "LM-DEF-004",
                error.to_string(),
                source,
                span,
            ));
            None
        }
    };

    let language = language.filter(|_| diagnostics.is_empty());
    DefinitionResult {
        language,
        diagnostics,
    }
}

fn lower_construct(
    node: &ParsedConstruct,
    source: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ConstructSpec> {
    let Some(name) = string_field(node, "name").and_then(decode_quoted) else {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-003",
            "construct declaration requires a valid quoted name",
            source,
            node.span,
        ));
        return None;
    };

    let syntax_nodes: Vec<_> = node
        .children
        .iter()
        .filter(|child| child.name == "syntax")
        .collect();
    if syntax_nodes.is_empty() {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-005",
            format!("construct `{name}` must have at least one `syntax` block"),
            source,
            node.span,
        ));
        return None;
    }

    let mut patterns = Vec::new();
    for syntax in syntax_nodes {
        let mut pattern = Vec::new();
        for part in &syntax.children {
            if let Some(pattern_part) = lower_pattern(part, source, diagnostics) {
                pattern.push(pattern_part);
            }
        }
        patterns.push(pattern);
    }

    let block_nodes: Vec<_> = node
        .children
        .iter()
        .filter(|child| child.name == "block")
        .collect();
    if block_nodes.len() > 1 {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-007",
            format!("construct `{name}` may declare only one block"),
            source,
            block_nodes[1].span,
        ));
        return None;
    }

    let mut patterns = patterns.into_iter();
    let mut construct = ConstructSpec::new(name, patterns.next().unwrap_or_default());
    for pattern in patterns {
        construct = construct.variant(pattern);
    }
    if let Some(block) = block_nodes.first() {
        let (Some(open), Some(close)) = (
            string_field(block, "open").and_then(decode_quoted),
            string_field(block, "close").and_then(decode_quoted),
        ) else {
            diagnostics.push(Diagnostic::error(
                "LM-DEF-008",
                "block requires quoted opening and closing tokens",
                source,
                block.span,
            ));
            return None;
        };
        let mut children = Vec::new();
        for child in block.children.iter().filter(|child| child.name == "allow") {
            let name = string_field(child, "name").and_then(decode_quoted);
            if let Some(name) = name {
                children.push(name);
            } else {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-009",
                    "`allow` requires a valid quoted construct name",
                    source,
                    child.span,
                ));
            }
        }
        construct = construct.block_names(open, close, children);
    }
    Some(construct)
}

fn lower_pattern(
    node: &ParsedConstruct,
    source: &SourceFile,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<PatternPart> {
    match node.name.as_str() {
        "literal" | "identifier" | "integer" | "string" => {
            let Some(value) = string_field(node, "value").and_then(decode_quoted) else {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-006",
                    "syntax item requires a valid quoted value",
                    source,
                    node.span,
                ));
                return None;
            };
            Some(match node.name.as_str() {
                "literal" => PatternPart::literal(value),
                "identifier" => PatternPart::Field {
                    name: value,
                    kind: FieldKind::Identifier,
                },
                "integer" => PatternPart::Field {
                    name: value,
                    kind: FieldKind::Integer,
                },
                "string" => PatternPart::Field {
                    name: value,
                    kind: FieldKind::String,
                },
                _ => return None,
            })
        }
        "optional" | "zero_or_more" | "one_or_more" => {
            let parts = node
                .children
                .iter()
                .filter_map(|child| lower_pattern(child, source, diagnostics))
                .collect::<Vec<_>>();
            Some(match node.name.as_str() {
                "optional" => PatternPart::optional(parts),
                "zero_or_more" => PatternPart::zero_or_more(parts),
                "one_or_more" => PatternPart::one_or_more(parts),
                _ => return None,
            })
        }
        _ => None,
    }
}

fn string_field<'a>(node: &'a ParsedConstruct, name: &str) -> Option<&'a str> {
    node.fields
        .iter()
        .find(|field| field.name == name)
        .and_then(|field| match &field.value {
            CapturedValue::String(value) => Some(value.as_str()),
            _ => None,
        })
}

fn decode_quoted(value: &str) -> Option<String> {
    let inner = value.strip_prefix('"')?.strip_suffix('"')?;
    let mut decoded = String::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }
        match characters.next()? {
            '\\' => decoded.push('\\'),
            '"' => decoded.push('"'),
            'n' => decoded.push('\n'),
            'r' => decoded.push('\r'),
            't' => decoded.push('\t'),
            _ => return None,
        }
    }
    Some(decoded)
}

fn spec_error_construct(error: &SpecError) -> Option<&str> {
    match error {
        SpecError::EmptyConstructName | SpecError::NoConstructs | SpecError::EmptyLanguageName => {
            None
        }
        SpecError::DuplicateConstruct(name)
        | SpecError::EmptyPattern(name)
        | SpecError::EmptyLiteral(name)
        | SpecError::InvalidBlockDelimiter(name) => Some(name),
        SpecError::InvalidLiteral { construct, .. } | SpecError::EmptyFieldName(construct) => {
            Some(construct)
        }
        SpecError::DuplicateField { construct, .. }
        | SpecError::UnknownChildConstruct { construct, .. }
        | SpecError::DuplicateChildConstruct { construct, .. }
        | SpecError::EmptyPatternGroup(construct)
        | SpecError::EmptyOptional(construct)
        | SpecError::EmptyRepetition(construct)
        | SpecError::AmbiguousPatternBoundary(construct)
        | SpecError::AmbiguousVariants(construct) => Some(construct),
        SpecError::AmbiguousConstructs(left, _) => Some(left),
    }
}

fn definition_grammar() -> &'static LanguageSpec {
    static GRAMMAR: OnceLock<LanguageSpec> = OnceLock::new();
    GRAMMAR.get_or_init(|| {
        LanguageSpec::new(
            "Langmeld definition",
            [
                ConstructSpec::new(
                    "language",
                    [
                        PatternPart::literal("language"),
                        PatternPart::string("name"),
                    ],
                )
                .block("{", "}", &["construct"]),
                ConstructSpec::new(
                    "construct",
                    [
                        PatternPart::literal("construct"),
                        PatternPart::string("name"),
                    ],
                )
                .block("{", "}", &["syntax", "block"]),
                ConstructSpec::new("syntax", [PatternPart::literal("syntax")]).block(
                    "{",
                    "}",
                    &[
                        "literal",
                        "identifier",
                        "integer",
                        "string",
                        "optional",
                        "zero_or_more",
                        "one_or_more",
                    ],
                ),
                ConstructSpec::new(
                    "block",
                    [
                        PatternPart::literal("block"),
                        PatternPart::string("open"),
                        PatternPart::string("close"),
                    ],
                )
                .block("{", "}", &["allow"]),
                ConstructSpec::new("optional", [PatternPart::literal("optional")]).block(
                    "{",
                    "}",
                    &[
                        "literal",
                        "identifier",
                        "integer",
                        "string",
                        "optional",
                        "zero_or_more",
                        "one_or_more",
                    ],
                ),
                ConstructSpec::new("zero_or_more", [PatternPart::literal("zero_or_more")]).block(
                    "{",
                    "}",
                    &[
                        "literal",
                        "identifier",
                        "integer",
                        "string",
                        "optional",
                        "zero_or_more",
                        "one_or_more",
                    ],
                ),
                ConstructSpec::new("one_or_more", [PatternPart::literal("one_or_more")]).block(
                    "{",
                    "}",
                    &[
                        "literal",
                        "identifier",
                        "integer",
                        "string",
                        "optional",
                        "zero_or_more",
                        "one_or_more",
                    ],
                ),
                ConstructSpec::new(
                    "literal",
                    [
                        PatternPart::literal("literal"),
                        PatternPart::string("value"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "identifier",
                    [
                        PatternPart::literal("identifier"),
                        PatternPart::string("value"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "integer",
                    [
                        PatternPart::literal("integer"),
                        PatternPart::string("value"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "string",
                    [
                        PatternPart::literal("string"),
                        PatternPart::string("value"),
                        PatternPart::literal(";"),
                    ],
                ),
                ConstructSpec::new(
                    "allow",
                    [
                        PatternPart::literal("allow"),
                        PatternPart::string("name"),
                        PatternPart::literal(";"),
                    ],
                ),
            ],
        )
        .expect("the built-in language-definition grammar must remain valid")
    })
}
