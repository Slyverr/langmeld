//! `SyntaxKind` and its `rowan` binding.

use langmeld_syntax::{SyntaxKind, SyntaxLanguage, TextRange, TextSize};
use rowan::{GreenNodeBuilder, Language, SyntaxNode};

const FILE: SyntaxKind = SyntaxKind(0);
const WORD: SyntaxKind = SyntaxKind(1);
const SPACE: SyntaxKind = SyntaxKind(u16::MAX);

#[test]
fn every_kind_survives_the_round_trip_through_rowan() {
    for raw in 0..=u16::MAX {
        let kind = SyntaxKind(raw);
        let through = SyntaxLanguage::kind_from_raw(SyntaxLanguage::kind_to_raw(kind));
        assert_eq!(through, kind);
    }
}

#[test]
fn builds_a_lossless_tree() {
    let mut builder = GreenNodeBuilder::new();
    builder.start_node(SyntaxLanguage::kind_to_raw(FILE));
    for (kind, text) in [(WORD, "héllo"), (SPACE, " \r\n"), (WORD, "😀")] {
        builder.token(SyntaxLanguage::kind_to_raw(kind), text);
    }
    builder.finish_node();
    let root = SyntaxNode::<SyntaxLanguage>::new_root(builder.finish());

    assert_eq!(root.kind(), FILE);
    assert_eq!(root.text().to_string(), "héllo \r\n😀");
    let kinds: Vec<_> = root
        .children_with_tokens()
        .map(|child| child.kind())
        .collect();
    assert_eq!(kinds, [WORD, SPACE, WORD]);
    // `rowan` ranges are the same `TextRange` this crate re-exports.
    let range: TextRange = root.text_range();
    assert_eq!(range.len(), TextSize::of("héllo \r\n😀"));
}
