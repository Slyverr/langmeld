//! Properties of binding that hold for any program: renaming never changes what a name
//! means, binding never fails on text that is not a program, and a project that was
//! edited binds as a project made from the edited text does.

use langmeld_ide::bind::{Bindings, Project, RenameError};
use langmeld_meta::{Loaded, Source, load_language};
use langmeld_syntax::{FileId, TextSize};
use proptest::prelude::*;

const GRAMMAR: &str = r#"langmeld 1
language Names { extensions "names" }
tokens {
    skip ws = /[ \n]+/
    name    = preset ident(ascii)
    int     = preset int(dec)
}
rule File   = Item*
rule Item   = Let | Fn | Block | Show
rule Let    = "let" name:Name "=" value:Value ";"
rule Fn     = "fn" name:Name "(" param:Param? ")" body:Block
rule Param  = name:Name
rule Block  = "{" Item* "}"
rule Show   = "show" value:Value ";"
rule Value  = NameRef | int
rule Name   = name
rule NameRef = name

bind {
    scope File, Block, Fn
    Let.name   defines value after
    Fn.name    defines value hoisted in parent
    Param.name defines value
    NameRef    refers value
}
"#;

fn language() -> Loaded {
    load_language(Source::text("names.langmeld", GRAMMAR)).unwrap_or_else(|error| panic!("{error}"))
}

const NAMES: [&str; 4] = ["a", "b", "c", "d"];

/// A program of the language, made of the few names there are so that they clash.
fn program() -> impl Strategy<Value = String> {
    let name = prop::sample::select(NAMES.to_vec());
    let item = prop_oneof![
        (name.clone(), name.clone()).prop_map(|(n, v)| format!("let {n} = {v};")),
        name.clone().prop_map(|v| format!("show {v};")),
    ];
    let items = item.prop_recursive(3, 24, 4, move |inner| {
        let name = prop::sample::select(NAMES.to_vec());
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4)
                .prop_map(|items| format!("{{ {} }}", items.join(" "))),
            (name.clone(), name, prop::collection::vec(inner, 0..3))
                .prop_map(|(f, p, items)| format!("fn {f}({p}) {{ {} }}", items.join(" "))),
        ]
    });
    prop::collection::vec(items, 0..8).prop_map(|items| items.join("\n"))
}

fn meanings(bindings: &Bindings) -> (Vec<Option<usize>>, Vec<&'static str>) {
    let targets = bindings.references().iter().map(|r| r.target).collect();
    let mut codes: Vec<_> = bindings.diagnostics().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    (targets, codes)
}

proptest! {
    /// After a rename that is allowed, every use still means the definition it meant,
    /// and nothing else is reported that was not.
    #[test]
    fn a_rename_that_is_allowed_changes_no_meaning(
        text in program(),
        which in 0..16usize,
        new in prop::sample::select(vec!["a", "b", "c", "d", "e", "let", "9"]),
    ) {
        let language = language();
        let mut project = Project::new(&language);
        let file = project.set("a", text.clone()).expect("small");
        let bindings = project.bind();
        let definitions = bindings.definitions();
        prop_assume!(!definitions.is_empty());
        let target = &definitions[which % definitions.len()];
        let Ok(mut edits) = project.rename(&bindings, file, target.range.start(), new) else {
            return Ok(());
        };
        let mut edited = text.clone();
        edits.sort_by_key(|edit| std::cmp::Reverse(edit.range.start()));
        for edit in &edits {
            let range = usize::from(edit.range.start())..usize::from(edit.range.end());
            prop_assert_eq!(&edited[range.clone()], target.name.as_str());
            edited.replace_range(range, &edit.replacement);
        }
        let mut after = Project::new(&language);
        after.set("a", edited).expect("small");
        prop_assert_eq!(meanings(&after.bind()), meanings(&bindings));
    }

    /// Whatever the offset and the new name, asking to rename does not fail but with
    /// a reason.
    #[test]
    fn renaming_anywhere_is_answered(text in program(), at in 0u32..400, new in "[a-z0-9 ]{0,3}") {
        let language = language();
        let mut project = Project::new(&language);
        let file = project.set("a", text).expect("small");
        let bindings = project.bind();
        match project.rename(&bindings, file, TextSize::new(at), &new) {
            Ok(edits) => prop_assert!(!edits.is_empty()),
            Err(RenameError::NothingThere | RenameError::NotAName | RenameError::ChangesMeaning) => {}
            Err(other) => prop_assert!(false, "{other}"),
        }
    }

    /// Text that is not a program is bound without failing.
    #[test]
    fn any_text_binds(text in "[a-z0-9 ;={}()\"\\n]{0,80}") {
        let language = language();
        let mut project = Project::new(&language);
        project.set("a", text.clone());
        project.set("b", format!("use \"a\"; {text}"));
        let bindings = project.bind();
        let _ = (bindings.unused(), bindings.visible(FileId::new(0), TextSize::new(0), "value").count());
    }

    /// A file that was changed binds as a project made from the new text does.
    #[test]
    fn an_edited_project_binds_like_a_new_one(
        text in program(),
        start in 0usize..200,
        length in 0usize..20,
        inserted in prop::sample::select(vec!["", "x", " let a = b; ", "{", "}", "show a;", ";"]),
    ) {
        let language = language();
        let mut start = start.min(text.len());
        while !text.is_char_boundary(start) {
            start -= 1;
        }
        let end = (start + length).min(text.len());
        let mut edited = text.clone();
        edited.replace_range(start..end, inserted);

        let mut project = Project::new(&language);
        project.set("a", text);
        project.set("a", edited.clone());
        let mut fresh = Project::new(&language);
        fresh.set("a", edited);
        let (old, new) = (project.bind(), fresh.bind());
        prop_assert_eq!(old.definitions(), new.definitions());
        prop_assert_eq!(old.references(), new.references());
        prop_assert_eq!(old.diagnostics(), new.diagnostics());
    }
}
