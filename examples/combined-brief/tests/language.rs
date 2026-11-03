use brief_language::{
    RuntimeContext, analyze_sources, load_language, load_language_runtime, load_project,
    load_project_runtime, parse_source, render_markdown,
};
use langmeld::core::SourceFile;
use std::{env, fs, path::PathBuf};

#[test]
fn starter_definition_and_example_parse() {
    let language = load_language().expect("compiled language model loads");
    assert_eq!(language.spec().name, "Brief");
    let runtime_language = load_language_runtime().expect("runtime language model loads");
    assert_eq!(language.spec(), runtime_language.spec());
    let compiled_project = load_project().expect("compiled project model loads");
    let runtime_project = load_project_runtime().expect("runtime project model loads");
    assert_eq!(
        compiled_project.language.spec(),
        runtime_project.language.spec()
    );

    let source = SourceFile::new(
        "hello.brief.lmprog",
        include_str!("../examples/hello.brief.lmprog"),
    );
    let parsed = parse_source(&source).expect("parser prepares");
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(parsed.program.nodes[0].construct, "program");
    assert_eq!(parsed.program.nodes[0].children[0].construct, "statement");
    assert_eq!(parsed.program.nodes[0].children[1].construct, "note");

    let sources = vec![source];
    let programs = vec![parsed.program];
    let analysis = analyze_sources(&language, &sources, &[&programs[0]])
        .expect("generic workspace analysis runs");
    assert!(
        analysis.diagnostics.is_empty(),
        "{:#?}",
        analysis.diagnostics
    );
    let markdown = render_markdown(&RuntimeContext {
        language: &language,
        sources: &sources,
        programs: &programs,
        analysis: &analysis,
    })
    .expect("host renders base and extension constructs");
    assert_eq!(markdown, "Hello from Brief!\n\n> A note from Brief.\n");
}

#[test]
#[ignore = "emits a Tree-sitter fixture from the composed Rust and .lm language"]
fn write_composed_tree_sitter_validation_fixture() {
    let output = env::var_os("LANGMELD_COMBINED_BRIEF_TREE_SITTER_FIXTURE_DIR")
        .expect("fixture output directory is configured");
    let output = PathBuf::from(output);
    fs::create_dir_all(&output).expect("fixture directory is created");
    let language = load_language_runtime().expect("effective extension language loads");
    for (path, contents) in langmeld::language::generate_tree_sitter(&language)
        .expect("composed Rust and .lm language grammar exports")
    {
        let relative = path
            .strip_prefix("tree-sitter/")
            .expect("artifacts are nested under tree-sitter");
        let path = output.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("artifact directory is created");
        }
        fs::write(path, contents).expect("Tree-sitter artifact is written");
    }
    let invalid = output.join("invalid/note-without-text.brief.lmprog");
    fs::create_dir_all(invalid.parent().expect("invalid fixture has a parent"))
        .expect("invalid fixture directory is created");
    fs::write(invalid, "program {\n  note\n}\n")
        .expect("invalid extension-construct fixture is written");
}
