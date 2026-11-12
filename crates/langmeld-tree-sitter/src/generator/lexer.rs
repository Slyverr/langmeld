//! Emit Tree-sitter lexical rules supported by declarative definitions.

use std::fmt::Write as _;

use langmeld_language::{FieldKind, LanguageSpec, PatternPart};

use super::js_string;

pub(super) fn write_lexer_rules(output: &mut String, language: &LanguageSpec) {
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
        writeln!(output, "    string: _ => /\"([^\"\\\\]|\\\\[\\s\\S])*\"/,")
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

pub(super) fn write_comment_rules(output: &mut String, language: &LanguageSpec) {
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
            PatternPart::Optional { parts: group, .. }
            | PatternPart::Repeat { parts: group, .. } => {
                collect_lexer_needs(group, needs_integer, needs_string, needs_punctuation);
            }
            PatternPart::Literal(_)
            | PatternPart::Choice { .. }
            | PatternPart::Expression { .. } => {}
        }
    }
}
