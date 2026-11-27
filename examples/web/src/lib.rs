//! The Calc language server for a browser: a web worker gives it each message as
//! JSON and posts the replies back to the page.

use calc::checks;
use langmeld::ide::{LanguageHooks, Server};
use langmeld::{Diagnostic, Parse, SourceFile};
use wasm_bindgen::prelude::wasm_bindgen;

struct Checks;

impl LanguageHooks for Checks {
    fn diagnostics(&self, file: &SourceFile, parsed: &Parse) -> Vec<Diagnostic> {
        if parsed.diagnostics.is_empty() {
            checks::divisions_by_zero(file, parsed)
        } else {
            Vec::new()
        }
    }
}

/// A language server for one page.
#[wasm_bindgen]
pub struct CalcServer(Server<'static, Checks>);

#[wasm_bindgen]
impl CalcServer {
    /// A server waiting to be initialized.
    #[wasm_bindgen(constructor)]
    #[allow(clippy::new_without_default)] // JavaScript calls `new`.
    pub fn new() -> Self {
        let loaded = calc::syntax::loaded();
        Self(Server::new(&loaded.language, &loaded.editor, Checks))
    }

    /// Takes a message of the protocol and gives the messages to post back.
    pub fn handle(&mut self, message: &str) -> Vec<String> {
        self.0.handle_json(message)
    }
}
