#![deny(missing_docs)]

//! Public Rust facade for Langmeld's language framework crates.
//!
//! The facade keeps APIs grouped by responsibility while letting consumers
//! depend on one package:
//!
//! - [`core`] defines source snapshots, diagnostics, schemas, and builders.
//! - [`expression`] provides configurable expression parsing and host rules.
//! - [`language`] provides lexing, parsing, analysis, formatting, and tooling.
//!
//! ```
//! use langmeld::core::{ConstructBuilder, LanguageBuilder, SourceFile};
//! use langmeld::language::PreparedLanguage;
//!
//! let language = LanguageBuilder::new("Greeting")
//!     .version("0.1.0")
//!     .root("greeting")
//!     .construct(
//!         ConstructBuilder::new("greeting")
//!             .syntax("greeting {")
//!             .block(["message"]),
//!     )
//!     .construct(ConstructBuilder::new("message").syntax("hello"))
//!     .build()
//!     .expect("valid language");
//! let parser = PreparedLanguage::new(language)
//!     .expect("language prepares")
//!     .with_custom_parsers([])
//!     .expect("declarative parser configures");
//! let parsed = parser.parse(&SourceFile::new("hello.demo", "greeting { hello }"));
//! assert!(parsed.diagnostics.is_empty());
//! ```

/// Source identities, diagnostics, schemas, and language builders.
pub use langmeld_core as core;
/// Configurable expression parsing, typing, and evaluation contracts.
pub use langmeld_expression as expression;
/// Generic language lexing, parsing, analysis, formatting, and tooling APIs.
pub use langmeld_language as language;
