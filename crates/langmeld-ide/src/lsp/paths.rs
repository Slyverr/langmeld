//! The addresses of documents and the paths of files.

use std::str::FromStr;

use lsp_types::Uri;

/// Whether `path` starts with a drive, such as `c:`.
fn drive(path: &str) -> bool {
    let mut bytes = path.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic()) && bytes.next() == Some(b':')
}

fn decoded(text: &str) -> String {
    let hex = |b: u8| char::from(b).to_digit(16);
    let mut bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    while let Some((&first, rest)) = bytes.split_first() {
        let escaped = match (first, rest) {
            (b'%', [high, low, ..]) => hex(*high).zip(hex(*low)),
            _ => None,
        };
        match escaped.and_then(|(high, low)| u8::try_from(high * 16 + low).ok()) {
            Some(byte) => {
                out.push(byte);
                bytes = rest.get(2..).unwrap_or_default();
            }
            None => {
                out.push(first);
                bytes = rest;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The path of the file at the address `uri`. An address of something other than a
/// file, such as an unsaved document, is its own path.
pub(crate) fn path_of(uri: &str) -> String {
    let Some(rest) = uri.strip_prefix("file://") else {
        return uri.to_owned();
    };
    let path = match rest.split_once('/') {
        Some(("" | "localhost", path)) => format!("/{path}"),
        // Another computer, as in a shared folder.
        _ => format!("//{rest}"),
    };
    let path = decoded(&path);
    match path.strip_prefix('/') {
        Some(inner) if drive(inner) => inner.to_owned(),
        _ => path,
    }
}

/// The address of the file at `path`, the reverse of [`path_of`].
pub(crate) fn uri_of(path: &str) -> Option<Uri> {
    let (start, rest) = if let Some(shared) = path.strip_prefix("//") {
        ("file://", shared)
    } else if path.starts_with('/') {
        ("file://", path)
    } else if drive(path) {
        ("file:///", path)
    } else {
        return Uri::from_str(path).ok();
    };
    let mut uri = start.to_owned();
    for byte in rest.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    Uri::from_str(&uri).ok()
}

#[cfg(test)]
mod tests {
    use super::{path_of, uri_of};

    #[test]
    fn file_addresses_and_paths_match() {
        for (uri, path) in [
            ("file:///work/a.x", "/work/a.x"),
            ("file:///work/with%20space/%C3%A9.x", "/work/with space/é.x"),
            ("file://server/share/a.x", "//server/share/a.x"),
            ("untitled:1", "untitled:1"),
        ] {
            assert_eq!(path_of(uri), path);
            assert_eq!(uri_of(path).expect("an address").as_str(), uri);
        }
        assert_eq!(path_of("file:///c%3A/work/a.x"), "c:/work/a.x");
        assert_eq!(path_of("file://localhost/a"), "/a");
        assert_eq!(path_of("file:///100%"), "/100%");
    }
}
