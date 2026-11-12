//! Build-time Rust projections for declarative Langmeld language definitions.
#![deny(missing_docs)]

mod codegen;
mod error;
mod project;

pub use codegen::{construct_type_name, generate_bindings};
pub use error::BuildError;
pub use project::generate_project;
