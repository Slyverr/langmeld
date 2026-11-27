//! Binding: files of any text bind, and renames are answered, with every kind of rule
//! about names and a resolver that computes names and looks members up.

#![no_main]

use std::sync::OnceLock;

use langmeld_ide::bind::{Member, MemberAccess, Project, Resolver};
use langmeld_ide::project::MemoryFiles;
use langmeld_meta::{Loaded, Source, load_language};
use langmeld_syntax::{SourceFile, SyntaxNode, TextRange, TextSize};
use libfuzzer_sys::fuzz_target;

/// A language with every rule about names.
const GRAMMAR: &str = r#"langmeld 1
language Names { extensions "n" }
tokens {
    skip ws = /[ \n]+/
    name    = preset ident(ascii)
    text    = preset string(double)
}
rule File    = Item*
rule Item    = Use | Let | Fn | Block | Group | Extend | Get | Show
rule Use     = "use" path:text ";"
rule Let     = "let" name:Name "=" value:NameRef ";"
rule Fn      = "fn" name:Name "(" param:Param? ")" body:Block
rule Param   = name:Name
rule Block   = "{" Item* "}"
rule Group   = "group" name:Name body:Block
rule Extend  = "extend" target:GroupRef body:Block
rule Get     = "get" object:NameRef "." member:Member ";"
rule Show    = "show" NameRef ";"
rule Name    = name
rule NameRef = name
rule GroupRef = name
rule Member  = name
bind {
    scope File, Block, Fn, Group
    Let.name    defines value after
    Fn.name     defines value hoisted in parent
    Param.name  defines value
    Group.name  defines group unique global computed
    NameRef     refers value
    GroupRef    refers group
    Use         imports value from Use.path
    Extend.body includes members of target
    Get.member  refers member of object
}
"#;

fn language() -> &'static Loaded {
    static LANGUAGE: OnceLock<Loaded> = OnceLock::new();
    LANGUAGE.get_or_init(|| {
        load_language(Source::text("names.langmeld", GRAMMAR)).expect("the grammar is fixed")
    })
}

/// Gives some groups two names, and looks members up among those of their owner.
struct Names;

impl Resolver for Names {
    fn predefined(&self) -> &[(&str, &str)] {
        &[("value", "print")]
    }

    fn computed_names(
        &self,
        _: &SourceFile,
        node: &SyntaxNode,
    ) -> Option<Vec<(String, TextRange)>> {
        let range = node.text_range();
        (u32::from(range.start()) % 2 == 0)
            .then(|| vec![("a".to_owned(), range), ("b".to_owned(), range)])
    }

    fn member(&self, access: &MemberAccess<'_>) -> Member {
        let bindings = access.bindings;
        let at = access.owner.text_range().start();
        match bindings.definition_index(access.file, at) {
            Some(owner) => bindings
                .members_of(owner)
                .find(|&member| bindings.definitions()[member].name == access.name)
                .map_or(Member::Missing, Member::Is),
            None => Member::Unknown,
        }
    }
}

/// Pieces of the language, so that random bytes make programs.
const PIECES: [&str; 24] = [
    "let ", "fn ", "group ", "extend ", "get ", "show ", "use ", "\"b\" ", "\"c\" ", "a ", "b ",
    "c ", "print ", "= ", "( ", ") ", "{ ", "} ", "; ", ". ", "\n", " ", "é", "\"",
];

const FILES: [&str; 3] = ["a", "b", "c"];

fuzz_target!(|data: &[u8]| {
    // The first bytes choose the offsets and names to try; the rest write the files.
    let Some((choices, rest)) = data.split_first_chunk::<8>() else {
        return;
    };
    let mut files = MemoryFiles::default();
    for (name, part) in FILES.into_iter().zip(rest.chunks(rest.len() / 3 + 1)) {
        let text: String = part
            .iter()
            .map(|&byte| PIECES[usize::from(byte) % PIECES.len()])
            .collect();
        files.insert(name, text);
    }
    let mut project = Project::new(language());
    for name in FILES {
        project.open(name, &files);
    }
    let bindings = Names.resolve(&project);
    let _ = bindings.unused();
    for diagnostic in bindings.diagnostics() {
        assert!(project.file(diagnostic.file).is_some());
    }
    for file in project.files() {
        let id = file.source().id();
        let length = file.source().text().len();
        for pair in choices.chunks_exact(2) {
            let at = usize::from(pair[0]) * (length + 1) / 256;
            let at = TextSize::try_from(at).expect("short");
            let _ = bindings.visible(id, at, "value").count();
            let _ = project.import_at(id, at);
            if let Some(definition) = bindings.definition_at(id, at) {
                let _ = bindings.references_to(definition).count();
            }
            let new = PIECES[usize::from(pair[1]) % PIECES.len()].trim();
            if let Ok(edits) = project.rename_with(&Names, &bindings, id, at, new) {
                assert!(!edits.is_empty());
            }
        }
    }
});
