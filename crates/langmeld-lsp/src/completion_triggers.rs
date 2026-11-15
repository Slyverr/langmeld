use std::collections::BTreeSet;

use langmeld_language::{LanguageSpec, PatternPart};

pub(crate) fn infer_completion_trigger_characters(language: &LanguageSpec) -> Vec<String> {
    let mut characters = BTreeSet::new();

    for construct in language.constructs() {
        for pattern in construct.variants() {
            for part in pattern {
                collect_trigger_characters(part, &mut characters);
            }
        }

        if let Some(block) = construct.block_spec() {
            collect_trigger_characters_from_text(block.open(), &mut characters);
            collect_trigger_characters_from_text(block.close(), &mut characters);
        }
    }

    characters
        .into_iter()
        .map(|character| character.to_string())
        .collect()
}

fn collect_trigger_characters(part: &PatternPart, characters: &mut BTreeSet<char>) {
    match part {
        PatternPart::Literal(value) => collect_trigger_characters_from_text(value, characters),
        PatternPart::Choice { values, .. } => {
            for value in values {
                collect_trigger_characters_from_text(value, characters);
            }
        }
        PatternPart::Expression { terminator, .. } => {
            collect_trigger_characters_from_text(terminator, characters);
        }
        PatternPart::Optional { parts, .. } | PatternPart::Repeat { parts, .. } => {
            for part in parts {
                collect_trigger_characters(part, characters);
            }
        }
        PatternPart::Field { .. } => {}
    }
}

fn collect_trigger_characters_from_text(value: &str, characters: &mut BTreeSet<char>) {
    characters.extend(value.chars().filter(|character| {
        !character.is_alphanumeric() && !character.is_whitespace() && *character != '_'
    }));
}
