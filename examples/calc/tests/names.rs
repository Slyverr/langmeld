//! Names across the files of a Calc program: where they are defined, what uses them, and
//! renaming them.

use calc::Builtins;
use calc::syntax::loaded;
use langmeld::bind::{Bindings, Project, RenameError, Resolver};
use langmeld::project::MemoryFiles;
use langmeld::{FileId, TextSize};

const LIB: &str = "fn twice(n) { n * 2 }\nlet base = 10;\n";
const MAIN: &str = "use \"lib.calc\";\nprint(twice(base));\nprint(twise(3));\nlet more = base;\n";

/// The project of the two files, with its names.
fn bound(main: &str) -> (Project<'static>, FileId, FileId, Bindings) {
    let mut files = MemoryFiles::default();
    files.insert("main.calc", main);
    files.insert("lib.calc", LIB);
    let mut project = Project::new(loaded());
    let main = project.open("main.calc", &files).expect("there");
    let lib = project.open("lib.calc", &files).expect("there");
    let bindings = Builtins.resolve(&project);
    (project, main, lib, bindings)
}

fn at(text: &str, word: &str, nth: usize) -> TextSize {
    let found = text.match_indices(word).nth(nth).expect("written").0;
    TextSize::new(u32::try_from(found).expect("small"))
}

#[test]
fn a_name_leads_to_its_definition_in_the_other_file() {
    let (project, main, lib, bindings) = bound(MAIN);
    let found = bindings
        .definition_at(main, at(MAIN, "twice", 0))
        .expect("defined");
    assert_eq!(found.file, lib);
    assert_eq!(found.range.start(), at(LIB, "twice", 0));
    // The path of an import leads to the file.
    assert_eq!(project.import_at(main, at(MAIN, "lib.calc", 0)), Some(lib));
}

#[test]
fn the_functions_that_exist_without_being_declared_are_not_reported() {
    let (_, _, _, bindings) = bound("print(abs(sqrt(4)));\n");
    assert!(bindings.diagnostics().is_empty());
}

#[test]
fn uses_are_found_in_the_file_that_imports() {
    let (_, main, lib, bindings) = bound(MAIN);
    let base = bindings
        .definition_at(lib, at(LIB, "base", 0))
        .expect("defined");
    let uses: Vec<_> = bindings
        .references_to(base)
        .map(|reference| (reference.file, reference.range.start()))
        .collect();
    assert_eq!(
        uses,
        [(main, at(MAIN, "base", 0)), (main, at(MAIN, "base", 1))]
    );
}

#[test]
fn an_unknown_name_gets_one_problem_that_says_what_it_may_be() {
    let (_, main, _, bindings) = bound(MAIN);
    let [found] = bindings.diagnostics() else {
        panic!("{:?}", bindings.diagnostics());
    };
    assert_eq!(found.code, "LM-B001");
    assert_eq!(found.file, main);
    assert_eq!(found.range.start(), at(MAIN, "twise", 0));
    assert_eq!(found.message, "`twise` is not defined");
    assert_eq!(found.notes, ["did you mean `twice`?"]);
    assert_eq!(found.fixes[0].title, "use `twice`");
    assert_eq!(found.fixes[0].edits[0].replacement, "twice");
}

#[test]
fn renaming_a_function_changes_both_files() {
    let (project, main, _, bindings) = bound(MAIN);
    let edits = project
        .rename_with(&Builtins, &bindings, main, at(MAIN, "twice", 0), "double")
        .expect("safe");
    let files: Vec<_> = edits.iter().map(|edit| edit.file).collect();
    assert_eq!(files.len(), 2);
    assert!(files.contains(&main) && files.iter().any(|&file| file != main));
    assert!(edits.iter().all(|edit| edit.replacement == "double"));
}

#[test]
fn renaming_is_refused_when_a_name_would_mean_something_else() {
    let main = "use \"lib.calc\";\nlet double = 2;\nprint(twice(double));\n";
    let (project, file, _, bindings) = bound(main);
    // `twice` would then be the `double` of this file.
    assert_eq!(
        project.rename_with(&Builtins, &bindings, file, at(main, "twice", 0), "double"),
        Err(RenameError::ChangesMeaning)
    );
    for bad in ["let", "1x", "a b", ""] {
        assert_eq!(
            project.rename_with(&Builtins, &bindings, file, at(main, "twice", 0), bad),
            Err(RenameError::NotAName),
            "{bad}"
        );
    }
}

#[test]
fn an_import_that_cannot_be_read_is_one_problem() {
    let mut files = MemoryFiles::default();
    files.insert("main.calc", "use \"gone.calc\";\nprint(1);\n");
    let mut project = Project::new(loaded());
    project.open("main.calc", &files).expect("there");
    let bindings = Builtins.resolve(&project);
    let [found] = bindings.diagnostics() else {
        panic!("{:?}", bindings.diagnostics());
    };
    assert_eq!(found.code, "LM-B003");
    assert_eq!(found.message, "the file `gone.calc` cannot be read");
}
