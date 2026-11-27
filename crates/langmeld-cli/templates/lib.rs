//! {{Name}}, a language built with Langmeld.
//!
//! The grammar is in `language/main.langmeld`; the build script turns it into the
//! types of [`syntax`]. What programs mean is in [`interp`].

pub mod cli;
pub mod interp;

/// The types of the syntax trees, and the language itself.
pub mod syntax {
    langmeld::include_syntax!();
}
