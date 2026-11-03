mod emit;
mod types;

pub use emit::generate_rust_ast;

use std::collections::BTreeSet;

fn unique(base: String, used: &mut BTreeSet<String>) -> String {
    if used.insert(base.clone()) {
        return base;
    }
    for suffix in 2_u32.. {
        let candidate = format!("{base}{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("an identifier suffix is always available")
}

fn pascal_case(value: &str) -> String {
    let mut result = String::new();
    let mut uppercase = true;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            result.push(if uppercase {
                character.to_ascii_uppercase()
            } else {
                character
            });
            uppercase = false;
        } else {
            uppercase = true;
        }
    }
    if result.is_empty() {
        result.push_str("Generated");
    }
    if result.as_bytes()[0].is_ascii_digit() || result == "Self" {
        result.insert_str(0, "Generated");
    }
    result
}

fn snake_case(value: &str) -> String {
    let mut result = String::new();
    let mut separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !result.is_empty() {
                result.push('_');
            }
            result.push(character.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    if result.is_empty() {
        result.push_str("field");
    }
    if result.as_bytes()[0].is_ascii_digit() {
        result.insert_str(0, "field_");
    }
    result
}

fn rust_keyword_safe(mut value: String) -> String {
    const KEYWORDS: &[&str] = &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "false", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "match", "mod",
        "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
        "trait", "true", "type", "unsafe", "use", "where", "while", "abstract", "become", "box",
        "do", "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
    ];
    if KEYWORDS.contains(&value.as_str()) {
        value.push('_');
    }
    value
}
