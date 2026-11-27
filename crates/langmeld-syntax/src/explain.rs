//! The long explanation of each diagnostic code, for people who want more than the
//! message.

const TEXT: &str = include_str!("explanations.md");

/// Each code with its explanation.
fn sections() -> impl Iterator<Item = (&'static str, &'static str)> {
    TEXT.split("## ").filter_map(|section| {
        let (code, text) = section.split_once('\n')?;
        Some((code.trim(), text.trim()))
    })
}

/// Every code Langmeld reports.
pub fn codes() -> impl Iterator<Item = &'static str> {
    sections().map(|(code, _)| code)
}

/// What the diagnostic code `code` means and how to solve it, if Langmeld reports it.
pub fn explain(code: &str) -> Option<&'static str> {
    sections().find_map(|(known, text)| (known == code).then_some(text))
}
