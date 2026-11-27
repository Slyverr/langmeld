//! What a language server serves: a language together with the rules that travel with it.

use langmeld_meta::{BindRules, Editor, Loaded};
use langmeld_syntax::Language;

/// What no `editor { }` block says.
static NO_EDITOR: Editor = Editor { rules: Vec::new() };

/// What no `bind { }` block says.
static NO_BINDINGS: BindRules = BindRules { rules: Vec::new() };

/// A language together with the rules that travel with it: how its names bind, and what
/// it tells editors.
///
/// A language read from a `.langmeld` file carries the rules of its `bind` and `editor`
/// blocks, so a [`Loaded`] is served as it is. A language written in Rust, or with a
/// parser of its own, is a [`Language`], which has no rules until it is given some with
/// [`bindings`](Self::bindings) and [`editor`](Self::editor).
///
/// ```
/// use langmeld_ide::Served;
/// use langmeld_meta::{Source, load_language};
///
/// let loaded = load_language(Source::text(
///     "words.langmeld",
///     "langmeld 1\nlanguage Words { extensions \"w\" }\n\
///      tokens { skip space = /[ \\n]+/\n word = preset ident(ascii) }\n\
///      rule File = Word*\nrule Word = text:word\n",
/// ))?;
/// let served = Served::from(&loaded);
/// assert_eq!(served.language().name(), "Words");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Served<'l> {
    language: &'l Language,
    editor: &'l Editor,
    bindings: &'l BindRules,
}

impl<'l> Served<'l> {
    /// A language with no rules: its names do not bind, and it tells editors nothing
    /// beyond its grammar.
    pub fn new(language: &'l Language) -> Self {
        Self {
            language,
            editor: &NO_EDITOR,
            bindings: &NO_BINDINGS,
        }
    }

    /// Gives the language what it tells editors, as [`Loaded::editor`] has it.
    #[must_use]
    pub fn editor(mut self, editor: &'l Editor) -> Self {
        self.editor = editor;
        self
    }

    /// Gives the language the rules its names bind by, as [`Loaded::bindings`] has them.
    #[must_use]
    pub fn bindings(mut self, bindings: &'l BindRules) -> Self {
        self.bindings = bindings;
        self
    }

    /// The language.
    pub fn language(&self) -> &'l Language {
        self.language
    }

    /// What the language tells editors.
    pub fn editor_rules(&self) -> &'l Editor {
        self.editor
    }

    /// The rules its names bind by.
    pub fn bind_rules(&self) -> &'l BindRules {
        self.bindings
    }
}

impl<'l> From<&'l Language> for Served<'l> {
    fn from(language: &'l Language) -> Self {
        Self::new(language)
    }
}

impl<'l> From<&'l Loaded> for Served<'l> {
    fn from(loaded: &'l Loaded) -> Self {
        Self::new(&loaded.language)
            .editor(&loaded.editor)
            .bindings(&loaded.bindings)
    }
}
