//! Where the files of a project come from, and the project files of a language's
//! users.

mod manifest;

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::Path;

pub use manifest::{ManifestError, ProjectManifest, find_up, keys};

/// The largest file [`DiskFiles`] reads, in bytes.
const LARGEST: u64 = 16 << 20;

/// The most files, and the deepest folders, [`DiskFiles`] lists.
const MOST: usize = 10_000;
const DEEPEST: usize = 32;

/// Where the files of a project come from. Paths use `/`.
pub trait FileProvider {
    /// The text of the file at `path`, if there is one.
    fn read(&self, path: &str) -> Option<String>;

    /// The paths of the files under the folder `root` whose extension is one of
    /// `extensions`, in order. A provider that cannot list gives none.
    fn list(&self, root: &str, extensions: &[&str]) -> Vec<String> {
        let _ = (root, extensions);
        Vec::new()
    }
}

impl<T: FileProvider + ?Sized> FileProvider for Box<T> {
    fn read(&self, path: &str) -> Option<String> {
        (**self).read(path)
    }

    fn list(&self, root: &str, extensions: &[&str]) -> Vec<String> {
        (**self).list(root, extensions)
    }
}

fn has_extension(path: &str, extensions: &[&str]) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.')
        .is_some_and(|(_, found)| extensions.contains(&found))
}

/// Files held in memory, by path.
#[derive(Clone, Debug, Default)]
pub struct MemoryFiles(BTreeMap<String, String>);

impl MemoryFiles {
    /// Adds or replaces a file.
    pub fn insert(&mut self, path: impl Into<String>, text: impl Into<String>) {
        self.0.insert(path.into(), text.into());
    }

    /// Forgets a file.
    pub fn remove(&mut self, path: &str) {
        self.0.remove(path);
    }
}

impl FromIterator<(String, String)> for MemoryFiles {
    fn from_iter<T: IntoIterator<Item = (String, String)>>(files: T) -> Self {
        Self(files.into_iter().collect())
    }
}

impl FileProvider for MemoryFiles {
    fn read(&self, path: &str) -> Option<String> {
        self.0.get(path).cloned()
    }

    fn list(&self, root: &str, extensions: &[&str]) -> Vec<String> {
        let root = root.trim_end_matches('/');
        self.0
            .keys()
            .filter(|path| {
                let under = path
                    .strip_prefix(root)
                    .is_some_and(|rest| root.is_empty() || rest.starts_with('/'));
                under && has_extension(path, extensions)
            })
            .cloned()
            .collect()
    }
}

/// The files of this computer.
///
/// Only ordinary files of at most 16 MiB are read. Listing skips hidden folders,
/// `target` and `node_modules`, follows no links to folders, and stops after
/// 10,000 files.
#[derive(Clone, Copy, Debug, Default)]
pub struct DiskFiles;

impl FileProvider for DiskFiles {
    fn read(&self, path: &str) -> Option<String> {
        // Devices and pipes may never end.
        if !std::fs::metadata(path).ok()?.is_file() {
            return None;
        }
        let mut text = String::new();
        let file = std::fs::File::open(path).ok()?;
        file.take(LARGEST + 1).read_to_string(&mut text).ok()?;
        (text.len() as u64 <= LARGEST).then_some(text)
    }

    fn list(&self, root: &str, extensions: &[&str]) -> Vec<String> {
        let mut found = Vec::new();
        let mut folders = vec![(Path::new(root).to_path_buf(), 0)];
        while let Some((folder, depth)) = folders.pop() {
            let Ok(entries) = std::fs::read_dir(folder) else {
                continue;
            };
            for entry in entries.flatten() {
                if found.len() >= MOST {
                    break;
                }
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if kind.is_dir() {
                    let skipped =
                        name.starts_with('.') || name == "target" || name == "node_modules";
                    if !skipped && depth < DEEPEST {
                        folders.push((entry.path(), depth + 1));
                    }
                } else if let Some(path) = entry.path().to_str() {
                    let path = path.replace('\\', "/");
                    if has_extension(&path, extensions) {
                        found.push(path);
                    }
                }
            }
        }
        found.sort_unstable();
        found
    }
}

/// Files from `over` where it has them, as an editor's unsaved texts, and from
/// `under` otherwise.
#[derive(Clone, Debug, Default)]
pub struct Overlay<A, B> {
    /// The files that win.
    pub over: A,
    /// The files beneath.
    pub under: B,
}

impl<A: FileProvider, B: FileProvider> FileProvider for Overlay<A, B> {
    fn read(&self, path: &str) -> Option<String> {
        self.over.read(path).or_else(|| self.under.read(path))
    }

    fn list(&self, root: &str, extensions: &[&str]) -> Vec<String> {
        let mut found = self.over.list(root, extensions);
        found.extend(self.under.list(root, extensions));
        found.sort_unstable();
        found.dedup();
        found
    }
}

/// `path` as the file at `from` means it: relative to its folder, with `.` and `..`
/// worked out.
pub(crate) fn resolve(from: &str, path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut parts: Vec<&str> = if absolute {
        vec![""]
    } else {
        from.split('/').collect()
    };
    if !absolute {
        parts.pop();
    }
    for part in path.split('/') {
        match part {
            "." | "" => {}
            ".." if parts
                .last()
                .is_some_and(|&last| !last.is_empty() && last != "..") =>
            {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    fn paths_are_relative_to_the_file_that_names_them() {
        assert_eq!(resolve("main", "lib/a"), "lib/a");
        assert_eq!(resolve("src/main", "a"), "src/a");
        assert_eq!(resolve("src/main", "./b/../a"), "src/a");
        assert_eq!(resolve("src/main", "../../a"), "../a");
        assert_eq!(resolve("/p/src/main", "../a"), "/p/a");
        assert_eq!(resolve("src/main", "/etc/a"), "/etc/a");
    }
}
