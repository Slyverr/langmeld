use langmeld::core::SourceFile;
use @CRATE@::{
    ast, load_language, load_language_runtime, load_project, load_project_runtime, parse_source,
};

#[test]
fn starter_definition_and_example_parse() {
    let language = load_language().expect("compiled language model loads");
    assert_eq!(language.spec().name, "@NAME@");
    let runtime_language = load_language_runtime().expect("runtime language model loads");
    assert_eq!(language.spec(), runtime_language.spec());
    let compiled_project = load_project().expect("compiled project model loads");
    let runtime_project = load_project_runtime().expect("runtime project model loads");
    assert_eq!(compiled_project.language.spec(), runtime_project.language.spec());

    let source = SourceFile::new(
        "hello.@EXT@",
        include_str!("../examples/hello.@EXT@"),
    );
    let parsed = parse_source(&source).expect("parser prepares");
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(parsed.program.nodes[0].construct, "program");
    assert_eq!(parsed.program.nodes[0].children[0].construct, "statement");
    let roots = ast::from_program(&parsed.program).expect("typed AST conversion succeeds");
    assert!(!roots[0].children.is_empty());
    let statement = ast::Statement::try_from(&parsed.program.nodes[0].children[0])
        .expect("starter child converts to a statement");
    assert_eq!(statement.message.value, "Hello from @NAME@!");
}
