//! Canonical formatting for Langmeld's built-in language-definition syntax.

use langmeld_core::{Diagnostic, SourceFile};

use crate::{
    DefaultLexer, LanguageSpec, Lexer, ParsedConstruct, TokenKind, language_definition_grammar,
    parse,
};

/// Indentation settings for formatting a `.langmeld` definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefinitionFormatOptions {
    /// Spaces per indentation level when `insert_spaces` is true, clamped to
    /// the range `1..=16` to bound output size.
    pub tab_size: u32,
    /// Use spaces instead of tab characters for indentation.
    pub insert_spaces: bool,
}

impl Default for DefinitionFormatOptions {
    fn default() -> Self {
        Self {
            tab_size: 4,
            insert_spaces: true,
        }
    }
}

/// Formats one syntactically valid `.langmeld` definition source.
///
/// String tokens are preserved exactly. The current `.langmeld` grammar does not
/// accept comments. Invalid or incomplete definitions return their parser
/// diagnostics rather than a potentially destructive partial rewrite.
///
/// # Errors
///
/// Returns source-located diagnostics when parsing fails or the parsed
/// definition cannot be formatted consistently with its validated grammar.
pub fn format_language_definition(
    source: &SourceFile,
    options: DefinitionFormatOptions,
) -> Result<String, Vec<Diagnostic>> {
    let grammar = language_definition_grammar();
    let parsed = parse(source, grammar);
    if !parsed.diagnostics().is_empty() {
        return Err(parsed.diagnostics().to_vec());
    }

    let tokens = DefaultLexer.lex(source).tokens;
    let indent_unit = if options.insert_spaces {
        " ".repeat(options.tab_size.clamp(1, 16) as usize)
    } else {
        "\t".to_owned()
    };
    let mut output = String::new();
    for (index, construct) in parsed.constructs().iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        format_construct(grammar, &tokens, construct, 0, &indent_unit, &mut output)
            .map_err(|message| vec![Diagnostic::spec_error("LM-FMT-001", message)])?;
    }
    if !output.is_empty() {
        output.push('\n');
    }
    Ok(output)
}

fn format_construct(
    grammar: &LanguageSpec,
    tokens: &[crate::LexedToken],
    construct: &ParsedConstruct,
    depth: usize,
    indent_unit: &str,
    output: &mut String,
) -> Result<(), &'static str> {
    let node_tokens = tokens
        .iter()
        .filter(|token| {
            token.span.start >= construct.span.start && token.span.end <= construct.span.end
        })
        .collect::<Vec<_>>();
    let block = grammar
        .constructs()
        .iter()
        .find(|spec| spec.name() == construct.name)
        .and_then(|spec| spec.block_spec());

    let Some(block) = block else {
        write_indent(output, depth, indent_unit);
        format_tokens(&node_tokens, output);
        return Ok(());
    };

    let Some(open_index) = node_tokens
        .iter()
        .position(|token| token.kind == TokenKind::Punctuation && token.text == block.open())
    else {
        return Err("parsed block construct is missing its opening delimiter");
    };
    write_indent(output, depth, indent_unit);
    format_tokens(&node_tokens[..open_index], output);
    output.push(' ');
    output.push_str(block.open());

    if construct.children.is_empty() {
        output.push_str(block.close());
        return Ok(());
    }

    output.push('\n');
    for (index, child) in construct.children.iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        format_construct(grammar, tokens, child, depth + 1, indent_unit, output)?;
    }
    output.push('\n');
    write_indent(output, depth, indent_unit);
    output.push_str(block.close());
    Ok(())
}

fn format_tokens(tokens: &[&crate::LexedToken], output: &mut String) {
    for token in tokens {
        match token.text.as_str() {
            ":" => {
                output.push_str(" :");
            }
            ";" | "," | ")" | "]" => {
                output.push_str(&token.text);
            }
            "(" | "[" => {
                output.push_str(&token.text);
            }
            _ => {
                if !output.is_empty() && !output.ends_with([' ', '\n', '\t']) {
                    output.push(' ');
                }
                output.push_str(&token.text);
            }
        }
        if token.text == ":" {
            output.push(' ');
        }
    }
}

fn write_indent(output: &mut String, depth: usize, indent_unit: &str) {
    for _ in 0..depth {
        output.push_str(indent_unit);
    }
}
