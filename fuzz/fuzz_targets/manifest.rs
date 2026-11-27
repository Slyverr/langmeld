//! `langmeld.toml` text of any kind: reading never panics, and every refusal is a
//! diagnostic with a valid place.

#![no_main]

use langmeld_meta::Manifest;
use libfuzzer_sys::fuzz_target;

/// Pieces of a manifest, so that random bytes reach deep into the reader.
const PIECES: &[&str] = &[
    "root = \"a.langmeld\"\n",
    "[bins]\n",
    "layout = \"split\"\n",
    "layout = \"other\"\n",
    "server = [\"x\", \"lsp\"]\n",
    "[project]\n",
    "manifest = \"x.toml\"\n",
    "[package.vscode]\n",
    "display-name = \"X\"\n",
    "publisher = \"p\"\n",
    "icon = 3\n",
    "extensions = [\"x\"]\n",
    "[language]\n",
    "[[bins]]\n",
    "\"",
    "'",
    "[",
    "]",
    "=",
    ".",
    "\\",
    "#",
    "\n",
    "@",
];

fn check(text: &str) {
    match Manifest::parse_text("langmeld.toml", text) {
        Ok(_) => {}
        Err(problems) => {
            assert!(!problems.is_empty(), "a refusal says why");
            for problem in problems {
                assert!(problem.range.start() <= problem.range.end());
                assert!(
                    usize::from(problem.range.end()) <= text.len(),
                    "{problem:?}"
                );
                assert!(
                    problem
                        .notes
                        .iter()
                        .any(|note| note.starts_with("To fix this"))
                );
            }
        }
    }
}

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        check(text);
    }
    let built: String = data
        .iter()
        .map(|&byte| PIECES[usize::from(byte) % PIECES.len()])
        .collect();
    check(&built);
});
