//! The Shapes language, for tests of the types written for its trees.
//!
//! When the grammar gains an alternative, code that matches on every kind of expression
//! stops compiling, with the error `E0004`:
//!
//! ```compile_fail,E0004
//! use typed::syntax::Expr;
//!
//! fn describe(expr: &Expr) -> &'static str {
//!     match expr {
//!         Expr::Num(_) => "number",
//!         Expr::Ref(_) => "name",
//!         Expr::Paren(_) => "group",
//!         Expr::Neg(_) => "negation",
//!         Expr::Mul(_) => "product",
//!     }
//! }
//! ```
//!
//! Code that covers every kind compiles:
//!
//! ```
//! use typed::syntax::Expr;
//!
//! fn describe(expr: &Expr) -> &'static str {
//!     match expr {
//!         Expr::Num(_) => "number",
//!         Expr::Ref(_) => "name",
//!         Expr::Paren(_) => "group",
//!         Expr::Neg(_) => "negation",
//!         Expr::Mul(_) => "product",
//!         Expr::Add(_) => "sum",
//!     }
//! }
//! # let _ = describe;
//! ```

/// The types of the syntax trees, and the language itself.
pub mod syntax {
    langmeld::include_syntax!();
}
