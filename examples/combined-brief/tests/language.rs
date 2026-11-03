use brief_language::{
    RuntimeContext, analyze_sources, load_language, load_language_runtime, load_project,
    load_project_runtime, parse_source, render_markdown,
};
use langmeld::core::SourceFile;

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
