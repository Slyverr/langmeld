//! Generate Tree-sitter grammars from declarative language definitions.

use std::fmt::Write as _;

use langmeld_language::LanguageSpec;

use super::TreeSitterError;

mod grammar;
mod lexer;

use grammar::{choice, construct_expression, construct_rule_name, rule_reference};
use lexer::{write_comment_rules, write_lexer_rules};

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
