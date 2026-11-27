//! Renaming a name in every file, when that changes nothing else.

use std::error::Error;
use std::fmt;

use langmeld_syntax::{
    FileId, SourceFile, SyntaxElement, TextEdit, TextRange, TextSize, TokenClass,
};

use super::{Bindings, Project, Resolver};
use crate::Served;

/// Why a name cannot be renamed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RenameError {
    /// No name is written there.
    NothingThere,
    /// The name is computed from other text, so there is nothing to rewrite.
    Computed,
    /// The new text is not a name of the same kind, or is a reserved word.
    NotAName,
    /// With the new name, some use would mean something else, or nothing.
    ChangesMeaning,
}

impl fmt::Display for RenameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NothingThere => "there is no name here",
            Self::Computed => "this name is computed, so it cannot be renamed",
            Self::NotAName => "the new text is not a name that can stand here",
            Self::ChangesMeaning => "with this name, other names would change what they mean",
        })
    }
}

impl Error for RenameError {}

/// Whether `new` reads as one token of the same kind as the name at `range`.
fn is_name_like(project: &Project<'_>, file: FileId, range: TextRange, new: &str) -> bool {
    let language = project.language();
    let Some(file) = project.file(file) else {
        return false;
    };
    let old = file
        .parse()
        .syntax()
        .descendants_with_tokens()
        .filter_map(SyntaxElement::into_token)
        .find(|token| token.text_range() == range);
    let Ok(alone) = SourceFile::new(FileId::new(0), "name", new) else {
        return false;
    };
    let lexed = language.lexer().lex(&alone);
    let [token] = lexed.tokens.as_slice() else {
        return false;
    };
    lexed.diagnostics.is_empty()
        && language.token_class(token.kind) != Some(TokenClass::Keyword)
        && old.is_some_and(|old| old.kind() == token.kind)
}

/// What every use means, as places that stay the same when names change.
fn meanings(bindings: &Bindings) -> Vec<Option<usize>> {
    bindings.references().iter().map(|r| r.target).collect()
}

fn codes(bindings: &Bindings) -> Vec<&'static str> {
    let mut codes: Vec<_> = bindings.diagnostics().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

pub(super) fn rename(
    project: &Project<'_>,
    resolver: &dyn Resolver,
    bindings: &Bindings,
    file: FileId,
    offset: TextSize,
    new: &str,
) -> Result<Vec<TextEdit>, RenameError> {
    let index = bindings
        .definition_index(file, offset)
        .ok_or(RenameError::NothingThere)?;
    let definition = bindings
        .definitions()
        .get(index)
        .ok_or(RenameError::NothingThere)?;
    if definition.computed {
        return Err(RenameError::Computed);
    }
    if !is_name_like(project, definition.file, definition.range, new) {
        return Err(RenameError::NotAName);
    }
    let edit = |file, range| TextEdit::new(file, range, new);
    let mut edits = vec![edit(definition.file, definition.range)];
    edits.extend(
        bindings
            .references()
            .iter()
            .filter(|reference| reference.target == Some(index))
            .map(|reference| edit(reference.file, reference.range)),
    );
    edits.sort_by_key(|edit| (edit.file, edit.range.start()));
    edits.dedup();

    // The same project with the edits made must mean what this one means.
    let mut changed = Project::new(Served::new(project.language()).bindings(project.rules()))
        .largest_file(usize::MAX);
    for old in project.files() {
        let mut text = old.source().text().to_owned();
        let here = edits.iter().filter(|edit| edit.file == old.source().id());
        for edit in here.rev() {
            let range = usize::from(edit.range.start())..usize::from(edit.range.end());
            // Places that a resolver of the language made up may not be text.
            if !text.is_char_boundary(range.start)
                || !text.is_char_boundary(range.end)
                || range.start > range.end
            {
                return Err(RenameError::ChangesMeaning);
            }
            text.replace_range(range, &edit.replacement);
        }
        if changed.set(old.source().name(), text).is_none() {
            return Err(RenameError::ChangesMeaning);
        }
    }
    let after = resolver.resolve(&changed);
    if meanings(&after) != meanings(bindings) || codes(&after) != codes(bindings) {
        return Err(RenameError::ChangesMeaning);
    }
    Ok(edits)
}
