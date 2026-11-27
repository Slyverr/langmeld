//! The project file of a language's users, such as `app.toml`: finding it, reading
//! it into the language's own type, and knowing which keys it can hold.

use std::cell::Cell;
use std::error::Error;
use std::fmt;

use langmeld_syntax::{Diagnostic, FileId, SourceFile, TextRange, TextSize};
use serde::de::value::BorrowedStrDeserializer;
use serde::de::{self, DeserializeOwned, DeserializeSeed, Deserializer, Visitor};

use super::FileProvider;

/// The path of the file `name` in the folder of `from`, or in the nearest folder
/// above it that has one.
pub fn find_up(files: &dyn FileProvider, from: &str, name: &str) -> Option<String> {
    let mut folder = from;
    while let Some((above, _)) = folder.rsplit_once('/') {
        let path = format!("{above}/{name}");
        if files.read(&path).is_some() {
            return Some(path);
        }
        folder = above;
    }
    // A relative path ends in the folder it is relative to.
    let relative = !from.starts_with('/') && from.contains('/');
    (relative && files.read(name).is_some()).then(|| name.to_owned())
}

/// A user's project file, read into the language's own type.
///
/// ```
/// use langmeld_ide::project::ProjectManifest;
/// use langmeld_syntax::{FileId, SourceFile};
///
/// #[derive(serde::Deserialize)]
/// struct App {
///     name: String,
/// }
///
/// let file = SourceFile::new(FileId::new(0), "app.toml", "name = \"demo\"")?;
/// let manifest = ProjectManifest::<App>::parse(file)?;
/// assert_eq!(manifest.value.name, "demo");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub struct ProjectManifest<T> {
    /// The file it was read from.
    pub file: SourceFile,
    /// What it says.
    pub value: T,
}

/// A project file that could not be read into the language's type.
#[derive(Debug)]
#[non_exhaustive]
pub struct ManifestError {
    /// The file, to show the problem in.
    pub file: SourceFile,
    /// The problem.
    pub diagnostic: Diagnostic,
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.file.name())?;
        let at = self.diagnostic.range.start();
        if let Some(at) = self.file.line_index().line_col(at) {
            write!(f, ":{}", at.line + 1)?;
        }
        write!(f, ": {}", self.diagnostic.message)
    }
}

impl Error for ManifestError {}

impl<T: DeserializeOwned> ProjectManifest<T> {
    /// Reads a project file, or says where it is wrong.
    pub fn parse(file: SourceFile) -> Result<Self, Box<ManifestError>> {
        match toml::from_str(file.text()) {
            Ok(value) => Ok(Self { file, value }),
            Err(problem) => {
                let at = |offset: usize| {
                    TextSize::try_from(offset.min(file.text().len())).unwrap_or_default()
                };
                let span = problem.span().unwrap_or(0..0);
                let range = TextRange::new(at(span.start), at(span.end.max(span.start)));
                let message = problem.message().trim().replace('\n', ", ");
                let diagnostic = Diagnostic::error("LM-M005", file.id(), range, message).note(
                    "To fix this, correct the file at the place marked: check the spelling of \
                     the key and the kind of its value.",
                );
                Err(Box::new(ManifestError { file, diagnostic }))
            }
        }
    }

    /// Finds the file `name` for the file at `from`, as [`find_up`] does, and reads
    /// it. Its [`FileId`] is the largest there is, apart from those of a project.
    pub fn discover(
        files: &dyn FileProvider,
        from: &str,
        name: &str,
    ) -> Option<Result<Self, Box<ManifestError>>> {
        let path = find_up(files, from, name)?;
        let text = files.read(&path)?;
        let file = SourceFile::new(FileId::new(u32::MAX), path, text).ok()?;
        Some(Self::parse(file))
    }

    /// The folder the project file is in, which is the root of the project.
    pub fn folder(&self) -> &str {
        let name = self.file.name();
        name.rsplit_once('/').map_or("", |(folder, _)| folder)
    }
}

/// The keys that the table `table` of a project file of type `T` can hold: none for
/// the file itself, or the keys that lead to the table.
///
/// ```
/// #[derive(serde::Deserialize)]
/// struct App {
///     name: String,
///     build: Option<Build>,
/// }
/// #[derive(serde::Deserialize)]
/// struct Build {
///     jobs: u32,
/// }
///
/// use langmeld_ide::project::keys;
/// assert_eq!(keys::<App>(&[]), ["name", "build"]);
/// assert_eq!(keys::<App>(&["build"]), ["jobs"]);
/// ```
pub fn keys<T: DeserializeOwned>(table: &[&str]) -> Vec<&'static str> {
    let found = Cell::new(None);
    // Reading stops with an error as soon as the keys are seen.
    let _ = T::deserialize(Probe {
        table,
        found: &found,
    });
    found.get().map_or_else(Vec::new, <[_]>::to_vec)
}

/// Asks a type for the keys of one of its tables, by pretending to be a file that
/// holds only the way to that table.
#[derive(Clone, Copy)]
struct Probe<'a> {
    table: &'a [&'a str],
    found: &'a Cell<Option<&'static [&'static str]>>,
}

#[derive(Debug)]
struct Stop;

impl fmt::Display for Stop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("seen enough")
    }
}

impl Error for Stop {}

impl de::Error for Stop {
    fn custom<T: fmt::Display>(_: T) -> Self {
        Self
    }
}

impl<'de> Deserializer<'de> for Probe<'_> {
    type Error = Stop;

    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Stop> {
        Err(Stop)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Stop> {
        let Some((first, rest)) = self.table.split_first() else {
            self.found.set(Some(fields));
            return Err(Stop);
        };
        let &key = fields.iter().find(|&field| field == first).ok_or(Stop)?;
        visitor.visit_map(OneKey {
            key: Some(key),
            value: Probe {
                table: rest,
                found: self.found,
            },
        })
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        visitor.visit_some(self)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Stop> {
        visitor.visit_newtype_struct(self)
    }

    /// An array of tables holds tables of one type.
    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Stop> {
        visitor.visit_seq(OneItem(Some(self)))
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes
        byte_buf unit unit_struct tuple tuple_struct map enum identifier ignored_any
    }
}

struct OneKey<'a> {
    key: Option<&'static str>,
    value: Probe<'a>,
}

impl<'de> de::MapAccess<'de> for OneKey<'_> {
    type Error = Stop;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Stop> {
        self.key
            .take()
            .map(|key| seed.deserialize(BorrowedStrDeserializer::new(key)))
            .transpose()
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Stop> {
        seed.deserialize(self.value)
    }
}

struct OneItem<'a>(Option<Probe<'a>>);

impl<'de> de::SeqAccess<'de> for OneItem<'_> {
    type Error = Stop;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Stop> {
        self.0.take().map(|item| seed.deserialize(item)).transpose()
    }
}
