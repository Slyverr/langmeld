//! What a language server serves: a language together with what it tells editors.

use langmeld_meta::{Editor, Loaded};
use langmeld_syntax::Language;

/// What no `editor { }` block says.
static NO_EDITOR: Editor = Editor { rules: Vec::new() };

/// A language together with the rules that travel with it: what it tells editors.
///
/// A language read from a `.langmeld` file carries its `editor { }` rules, so a
/// [`Loaded`] is served as it is. A language written in Rust, or with a parser of its
/// own, is a [`Language`], which tells editors nothing until it is given rules with
/// [`editor`](Self::editor).
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
}

impl<'l> Served<'l> {
    /// A language that tells editors nothing beyond its grammar.
    pub fn new(language: &'l Language) -> Self {
        Self {
            language,
            editor: &NO_EDITOR,
        }
    }

    /// Gives the language what it tells editors, as [`Loaded::editor`] has it.
    #[must_use]
    pub fn editor(mut self, editor: &'l Editor) -> Self {
        self.editor = editor;
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
}

impl<'l> From<&'l Language> for Served<'l> {
    fn from(language: &'l Language) -> Self {
        Self::new(language)
    }
}

impl<'l> From<&'l Loaded> for Served<'l> {
    fn from(loaded: &'l Loaded) -> Self {
        Self::new(&loaded.language).editor(&loaded.editor)
    }
}
