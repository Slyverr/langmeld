//! Lossless syntax structure for the declarative parser.

mod builder;
mod model;

pub(crate) use builder::{build_syntax_tree, unparsed_syntax_tree};
pub use model::{
    SyntaxElement, SyntaxNode, SyntaxNodeKind, SyntaxToken, SyntaxTokenKind, SyntaxTree,
};
