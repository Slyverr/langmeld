//! The Rust code of a language's `extern` declarations, kept by name.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::{ExternRule, LexCursor, Parser, Token};

type RuleCode = dyn Fn(&mut Parser<'_>) + Send + Sync;
type TokenCode = dyn Fn(&mut LexCursor<'_>) -> Option<usize> + Send + Sync;

/// The code of custom rules and tokens, by the names a grammar file declares them with.
/// A file cannot hold code, so whoever loads it gives the code here.
///
/// ```
/// use langmeld_syntax::Externals;
///
/// let code = Externals::new().rule("Block", |p| p.bump());
/// assert!(code.extern_rule("Block").is_some());
/// assert!(code.extern_rule("Other").is_none());
/// ```
#[derive(Clone, Default)]
pub struct Externals {
    rules: BTreeMap<String, Arc<RuleCode>>,
    tokens: BTreeMap<String, Arc<TokenCode>>,
}

impl Externals {
    /// No code, for a language that declares none.
    pub fn new() -> Self {
        Self::default()
    }

    /// The code of the custom rule called `name`.
    #[must_use]
    pub fn rule(
        mut self,
        name: impl Into<String>,
        parse: impl Fn(&mut Parser<'_>) + Send + Sync + 'static,
    ) -> Self {
        self.rules.insert(name.into(), Arc::new(parse));
        self
    }

    /// The code of the external token called `name`.
    #[must_use]
    pub fn token(
        mut self,
        name: impl Into<String>,
        lex: impl Fn(&mut LexCursor<'_>) -> Option<usize> + Send + Sync + 'static,
    ) -> Self {
        self.tokens.insert(name.into(), Arc::new(lex));
        self
    }

    pub(crate) fn rule_code(&self, name: &str) -> Option<Arc<RuleCode>> {
        self.rules.get(name).cloned()
    }

    pub(crate) fn token_code(&self, name: &str) -> Option<Arc<TokenCode>> {
        self.tokens.get(name).cloned()
    }

    /// The custom rule called `name`, if its code was given.
    pub fn extern_rule(&self, name: &str) -> Option<ExternRule> {
        let code = self.rules.get(name)?.clone();
        Some(ExternRule::new(name, move |p| code(p)))
    }

    /// The external token called `name`, if its code was given.
    pub fn extern_token(&self, name: &str) -> Option<Token> {
        let code = self.tokens.get(name)?.clone();
        Some(Token::external(name, move |cursor| code(cursor)))
    }
}

impl fmt::Debug for Externals {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Externals")
            .field("rules", &self.rules.keys())
            .field("tokens", &self.tokens.keys())
            .finish()
    }
}
