//! Writes a tree the way corpus files do. Included by test targets that import
//! `Parse`, `SyntaxKind` and `SyntaxNode` at their root.

use std::fmt::Write;

use crate::{Parse, SyntaxKind, SyntaxNode};

/// One node or token per line, without trivia; a node holding a single token, or
/// only unexpected ones, takes one line. Diagnostics follow, one per line.
pub fn render(
    parse: &Parse,
    name: &dyn Fn(SyntaxKind) -> String,
    is_trivia: &dyn Fn(SyntaxKind) -> bool,
) -> String {
    let mut out = String::new();
    node(&parse.syntax(), 0, name, is_trivia, &mut out);
    for d in &parse.diagnostics {
        let (start, end) = (u32::from(d.range.start()), u32::from(d.range.end()));
        writeln!(out, "! {} {start}..{end} {}", d.code, d.message).expect("writes to a string");
    }
    out.pop();
    out
}

fn node(
    current: &SyntaxNode,
    depth: usize,
    name: &dyn Fn(SyntaxKind) -> String,
    is_trivia: &dyn Fn(SyntaxKind) -> bool,
    out: &mut String,
) {
    let children: Vec<_> = current
        .children_with_tokens()
        .filter(|child| !is_trivia(child.kind()))
        .collect();
    let indent = "  ".repeat(depth);
    let called = name(current.kind());
    let only_tokens = children.iter().all(|child| child.as_token().is_some());
    let one_line = depth > 0
        && only_tokens
        && !children.is_empty()
        && (children.len() == 1 || called == "Error");
    write!(out, "{indent}{called}").expect("writes to a string");
    if one_line {
        for token in children.iter().filter_map(|child| child.as_token()) {
            write!(out, " {:?}", token.text()).expect("writes to a string");
        }
        out.push('\n');
        return;
    }
    out.push('\n');
    for child in children {
        if let Some(token) = child.as_token() {
            writeln!(out, "{indent}  {:?}", token.text()).expect("writes to a string");
        } else if let Some(child) = child.as_node() {
            node(child, depth + 1, name, is_trivia, out);
        }
    }
}
