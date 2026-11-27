//! The command line of a language: the commands every language needs, and room for its
//! own. The `langmeld` binary is built on the same parts.
//!
//! **Experimental:** the API may change in any minor release.
//!
//! ```no_run
//! use std::process::ExitCode;
//!
//! use langmeld_cli::App;
//! # fn language() -> langmeld_syntax::Language { unimplemented!() }
//!
//! fn main() -> ExitCode {
//!     let language = language();
//!     App::new(&language).name("mylang").standard().run()
//! }
//! ```
//!
//! A command ends with exit code 0 when all is well, 1 when it found problems and
//! showed them, and 2 when it could not do its work.

mod app;
mod files;
mod output;
mod source;
mod testing;

use std::fmt;

pub use app::App;
pub use clap;
pub use files::{Format, check_files, check_files_with};
pub use output::{error_line, line, report};
pub use source::{files_in, parse_file, read};
pub use testing::run_corpus;

/// Why a command did not succeed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Failure {
    /// Problems were found and have been shown. The exit code is 1.
    Found,
    /// The command could not do its work, for this reason. The exit code is 2.
    Unable(String),
}

impl Failure {
    /// The exit code of a command that ended this way.
    ///
    /// ```
    /// use langmeld_cli::Failure;
    ///
    /// assert_eq!(Failure::Found.code(), 1);
    /// assert_eq!(Failure::Unable("no such file".to_owned()).code(), 2);
    /// ```
    pub fn code(&self) -> u8 {
        match self {
            Self::Found => 1,
            Self::Unable(_) => 2,
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Found => f.write_str("problems were found"),
            Self::Unable(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for Failure {}
