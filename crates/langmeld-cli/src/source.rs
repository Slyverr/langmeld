//! Reading the files a command works on.

use std::path::{Path, PathBuf};

use langmeld_syntax::{FileId, Language, Parse, SourceFile};

use crate::Failure;

/// Reads a text file.
pub fn read(path: &Path) -> Result<String, Failure> {
    std::fs::read_to_string(path)
        .map_err(|problem| Failure::Unable(format!("cannot read {}: {problem}", path.display())))
}

/// Reads and parses a file of a language. `id` numbers the file among those read.
pub fn parse_file(
    language: &Language,
    path: &Path,
    id: FileId,
) -> Result<(SourceFile, Parse), Failure> {
    let file = SourceFile::new(id, path.display().to_string(), read(path)?)
        .map_err(|problem| Failure::Unable(format!("{}: {problem}", path.display())))?;
    let parsed = langmeld_syntax::parse(language, &file);
    Ok((file, parsed))
}

/// How deep folders are searched, which stops loops made by links.
const DEPTH: usize = 64;

fn search(
    folder: &Path,
    extensions: &[&str],
    depth: usize,
    into: &mut Vec<PathBuf>,
) -> Result<(), Failure> {
    let unable =
        |problem: std::io::Error| Failure::Unable(format!("{}: {problem}", folder.display()));
    for entry in std::fs::read_dir(folder).map_err(unable)? {
        let entry = entry.map_err(unable)?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Hidden files and build output are not the project's sources.
        if name.starts_with('.') || name == "target" {
            continue;
        }
        if path.is_dir() {
            if depth < DEPTH {
                search(&path, extensions, depth + 1, into)?;
            }
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extensions.contains(&extension))
        {
            into.push(path);
        }
    }
    Ok(())
}

/// The files that `paths` name: each file given, and in each folder the files with an
/// extension of `extensions`, in name order. Hidden folders and `target` are passed
/// over.
pub fn files_in(language: &Language, paths: &[PathBuf]) -> Result<Vec<PathBuf>, Failure> {
    let extensions: Vec<&str> = language.extensions().collect();
    let mut files = Vec::new();
    for path in paths {
        if !path.is_dir() {
            if !path.exists() {
                return Err(Failure::Unable(format!(
                    "{} is not a file or a folder",
                    path.display()
                )));
            }
            files.push(path.clone());
            continue;
        }
        if extensions.is_empty() {
            return Err(Failure::Unable(format!(
                "{} declares no file extension, so name its files instead of folders",
                language.name()
            )));
        }
        let first = files.len();
        search(path, &extensions, 0, &mut files)?;
        if let Some(found) = files.get_mut(first..) {
            found.sort();
        }
    }
    if files.is_empty() {
        let listed: Vec<_> = extensions.iter().map(|e| format!(".{e}")).collect();
        return Err(Failure::Unable(format!(
            "no {} files found",
            listed.join(" or ")
        )));
    }
    Ok(files)
}
