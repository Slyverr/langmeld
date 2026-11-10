//! Tree-sitter grammar generation from declarative Langmeld languages.
#![deny(missing_docs)]

use std::fmt::Write as _;

use langmeld_language::{ConstructSpec, FieldKind, LanguageSpec, PatternPart, RepeatMinimum};

/// Options that identify an exported Tree-sitter grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeSitterOptions {
    grammar_name: String,
}

impl TreeSitterOptions {
    /// Creates export options with a Tree-sitter grammar name.
    ///
    /// Names must begin with an ASCII lowercase letter or underscore and
    /// contain only lowercase ASCII letters, digits, and underscores.
    #[must_use]
    pub fn new(grammar_name: impl Into<String>) -> Self {
        Self {
            grammar_name: grammar_name.into(),
        }
    }

    /// Returns the Tree-sitter grammar name.
    #[must_use]
    pub fn grammar_name(&self) -> &str {
        &self.grammar_name
    }
}

/// Generates a Tree-sitter `grammar.js` source file.
///
/// The output represents the supplied declarative syntax using Tree-sitter's
/// grammar DSL. It uses the built-in lexical categories for identifiers,
/// decimal integers, quoted strings, and grouped punctuation. It cannot infer
/// host-defined lexer behavior, runtime semantics, or recovery policy; custom
/// parser languages need a host-authored Tree-sitter grammar.
pub fn generate(
    language: &LanguageSpec,
    options: &TreeSitterOptions,
) -> Result<String, TreeSitterError> {
    validate_options(options)?;
    let roots = language.root_constructs().collect::<Vec<_>>();
    if roots.is_empty() {
        return Err(TreeSitterError::NoRootConstructs);
    }
    if let Some((opening, closing)) = language.lexical_rules().block_comments().first() {
        return Err(TreeSitterError::UnsupportedBlockComment {
            opening: opening.clone(),
            closing: closing.clone(),
        });
    }

    let rules = language
        .constructs()
        .iter()
        .enumerate()
        .map(|(index, construct)| {
            (
                construct.name().to_owned(),
                construct_rule_name(index, construct.name()),
            )
        })
        .collect::<Vec<_>>();
    let mut output = String::new();
    writeln!(output, "module.exports = grammar({{").expect("writing to a string cannot fail");
    writeln!(output, "  name: {},", js_string(options.grammar_name()))
        .expect("writing to a string cannot fail");
    let mut extras = vec!["/\\s/".to_owned()];
    extras.extend(
        (0..language.lexical_rules().line_comments().len())
            .map(|index| format!("$.line_comment_{index}")),
    );
    writeln!(output, "  extras: $ => [{}],", extras.join(", "))
        .expect("writing to a string cannot fail");
    writeln!(output, "  word: $ => $.identifier,").expect("writing to a string cannot fail");
    writeln!(output, "  rules: {{").expect("writing to a string cannot fail");

    let root_rules = roots
        .iter()
        .map(|construct| {
            format!(
                "$.{}",
                rule_reference(&rules, construct.name())
                    .expect("validated root constructs exist in the language")
            )
        })
        .collect::<Vec<_>>();
    writeln!(
        output,
        "    source_file: $ => repeat({}),",
        choice(&root_rules)
    )
    .expect("writing to a string cannot fail");

    for construct in language.constructs() {
        let expression = construct_expression(construct, &rules)?;
        writeln!(
            output,
            "    {}: $ => {},",
            rule_reference(&rules, construct.name()).expect("construct has a generated rule"),
            expression
        )
        .expect("writing to a string cannot fail");
    }

    write_lexer_rules(&mut output, language);
    write_comment_rules(&mut output, language);
    writeln!(output, "  }},").expect("writing to a string cannot fail");
    writeln!(output, "}});").expect("writing to a string cannot fail");
    Ok(output)
}

fn validate_options(options: &TreeSitterOptions) -> Result<(), TreeSitterError> {
    let mut characters = options.grammar_name.chars();
    let valid_start = characters
        .next()
        .is_some_and(|character| character.is_ascii_lowercase() || character == '_');
    if !valid_start
        || !characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
    {
        return Err(TreeSitterError::InvalidGrammarName(
            options.grammar_name.clone(),
        ));
    }
    Ok(())
}

fn construct_rule_name(index: usize, name: &str) -> String {
    let mut fragment = String::new();
    let mut separator = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !fragment.is_empty() {
                fragment.push('_');
            }
            fragment.push(character.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    if fragment.is_empty() {
        fragment.push_str("item");
    }
    format!("construct_{index}_{fragment}")
}

fn rule_reference<'a>(rules: &'a [(String, String)], name: &str) -> Option<&'a str> {
    rules
        .iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, rule)| rule.as_str())
}

fn construct_expression(
    construct: &ConstructSpec,
    rules: &[(String, String)],
) -> Result<String, TreeSitterError> {
    let variants = construct
        .variants()
        .iter()
        .map(|pattern| {
            let mut parts = vec![sequence(pattern, construct.name())?];
            if let Some(block) = construct.block_spec() {
                parts.push(js_string(block.open()));
                let children = block
                    .constructs()
                    .iter()
                    .map(|name| {
                        format!(
                            "$.{}",
                            rule_reference(rules, name)
                                .expect("validated block children exist in the language")
                        )
                    })
                    .collect::<Vec<_>>();
                if !children.is_empty() {
                    parts.push(format!("repeat({})", choice(&children)));
                }
                parts.push(js_string(block.close()));
            }
            Ok(sequence_strings(&parts))
        })
        .collect::<Result<Vec<_>, TreeSitterError>>()?;
    Ok(choice(&variants))
}

fn sequence(parts: &[PatternPart], construct: &str) -> Result<String, TreeSitterError> {
    let items = parts
        .iter()
        .map(|part| pattern_part(part, construct))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(sequence_strings(&items))
}

fn sequence_strings(parts: &[String]) -> String {
    match parts {
        [one] => one.clone(),
        _ => format!("seq({})", parts.join(", ")),
    }
}

fn pattern_part(part: &PatternPart, construct: &str) -> Result<String, TreeSitterError> {
    Ok(match part {
        PatternPart::Literal(literal) => js_string(literal),
        PatternPart::Field { name, kind } => {
            let symbol = match kind {
                FieldKind::Token => "token",
                FieldKind::Identifier => "identifier",
                FieldKind::Integer => "integer",
                FieldKind::String => "string",
            };
            format!("field({}, $.{symbol})", js_string(name))
        }
        PatternPart::Choice { name, values, .. } => {
            let alternatives = values
                .iter()
                .map(|value| js_string(value))
                .collect::<Vec<_>>();
            format!("field({}, {})", js_string(name), choice(&alternatives))
        }
        PatternPart::Expression { name, .. } => {
            return Err(TreeSitterError::UnsupportedExpressionCapture {
                construct: construct.to_owned(),
                field: name.clone(),
            });
        }
        PatternPart::Optional(parts) => format!("optional({})", sequence(parts, construct)?),
        PatternPart::Repeat { parts, minimum } => {
            let function = match minimum {
                RepeatMinimum::Zero => "repeat",
                RepeatMinimum::One => "repeat1",
            };
            format!("{function}({})", sequence(parts, construct)?)
        }
    })
}

fn choice(rules: &[String]) -> String {
    match rules {
        [one] => one.clone(),
        _ => format!("choice({})", rules.join(", ")),
    }
}

fn js_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                write!(output, "\\u{:04x}", character as u32)
                    .expect("writing to a string cannot fail");
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn write_lexer_rules(output: &mut String, language: &LanguageSpec) {
    writeln!(
        output,
        "    identifier: _ => /(?:\\p{{Alphabetic}}|_)(?:\\p{{Alphabetic}}|\\p{{Number}}|_)*/,"
    )
    .expect("writing to a string cannot fail");
    let mut needs_integer = false;
    let mut needs_string = false;
    let mut needs_punctuation = false;
    for construct in language.constructs() {
        for variant in construct.variants() {
            collect_lexer_needs(
                variant,
                &mut needs_integer,
                &mut needs_string,
                &mut needs_punctuation,
            );
        }
    }
    if needs_integer {
        writeln!(output, "    integer: _ => /[0-9]+/,").expect("writing to a string cannot fail");
    }
    if needs_string {
        writeln!(output, "    string: _ => /\"([^\"\\\\]|\\\\.)*\"/,")
            .expect("writing to a string cannot fail");
    }
    if needs_punctuation {
        writeln!(
            output,
            "    punctuation: _ => /[^\\p{{Alphabetic}}0-9_\\s\"]+/,"
        )
        .expect("writing to a string cannot fail");
        writeln!(
            output,
            "    token: $ => choice($.identifier, $.integer, $.string, $.punctuation),"
        )
        .expect("writing to a string cannot fail");
    }
}

fn write_comment_rules(output: &mut String, language: &LanguageSpec) {
    for (index, marker) in language.lexical_rules().line_comments().iter().enumerate() {
        writeln!(
            output,
            "    line_comment_{index}: _ => token(seq({}, /[^\\r\\n]*/)),",
            js_string(marker)
        )
        .expect("writing to a string cannot fail");
    }
}

fn collect_lexer_needs(
    parts: &[PatternPart],
    needs_integer: &mut bool,
    needs_string: &mut bool,
    needs_punctuation: &mut bool,
) {
    for part in parts {
        match part {
            PatternPart::Field { kind, .. } => match kind {
                FieldKind::Token => {
                    *needs_integer = true;
                    *needs_string = true;
                    *needs_punctuation = true;
                }
                FieldKind::Integer => *needs_integer = true,
                FieldKind::String => *needs_string = true,
                FieldKind::Identifier => {}
            },
            PatternPart::Optional(group) | PatternPart::Repeat { parts: group, .. } => {
                collect_lexer_needs(group, needs_integer, needs_string, needs_punctuation);
            }
            PatternPart::Literal(_)
            | PatternPart::Choice { .. }
            | PatternPart::Expression { .. } => {}
        }
    }
}

/// An error while creating a Tree-sitter grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeSitterError {
    /// The grammar name does not satisfy Tree-sitter's identifier format.
    InvalidGrammarName(String),
    /// The language has no root constructs from which to build a source rule.
    NoRootConstructs,
    /// A host-parsed expression capture cannot be represented by the exporter.
    UnsupportedExpressionCapture {
        /// Construct containing the unsupported capture.
        construct: String,
        /// Name of the expression field.
        field: String,
    },
    /// A configured block-comment rule cannot be represented by this exporter.
    UnsupportedBlockComment {
        /// Comment opening marker.
        opening: String,
        /// Comment closing marker.
        closing: String,
    },
}

impl std::fmt::Display for TreeSitterError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidGrammarName(name) => write!(
                formatter,
                "invalid Tree-sitter grammar name `{name}`; use lowercase ASCII letters, digits, or underscores and start with a letter or underscore"
            ),
            Self::NoRootConstructs => {
                formatter.write_str("cannot export a grammar with no root constructs")
            }
            Self::UnsupportedExpressionCapture { construct, field } => write!(
                formatter,
                "cannot export expression field `{field}` in construct `{construct}`; use a host-authored Tree-sitter grammar for expression syntax"
            ),
            Self::UnsupportedBlockComment { opening, closing } => write!(
                formatter,
                "cannot export block comments `{opening}` ... `{closing}` to Tree-sitter"
            ),
        }
    }
}

impl std::error::Error for TreeSitterError {}
