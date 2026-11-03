use langmeld::{
    core::{Severity, SourceFile},
    language::{AstNode, Value, classify_parse_result_prepared, load_project},
};
use python_source_language::{
    analyze_source, analyze_sources, configured_parser, generated, has_errors,
};

fn function<'a>(root: &'a AstNode, name: &str) -> &'a AstNode {
    root.children
        .iter()
        .find(|node| {
            node.construct == "function"
                && matches!(
                    node.fields.get("name").map(|field| &field.value),
                    Some(Value::Identifier(actual)) if actual == name
                )
        })
        .unwrap_or_else(|| panic!("Python function `{name}` is indexed"))
}

#[test]
fn cpython_ast_maps_parameters_annotations_and_direct_calls_to_typed_ast() {
    let source = SourceFile::new(
        "main.py",
        "async def main(first: str, /, *values: int, enabled: bool = True, **options: object) -> str:\n    return greet(first)\n\ndef greet(name: str) -> str:\n    return name\n",
    );
    let analysis = analyze_source(&source);
    assert!(!has_errors(&analysis), "{:?}", analysis.diagnostics);
    let root = &analysis.parsed.program.nodes[0];
    let main = function(root, "main");
    assert_eq!(main.fields["kind"].value, Value::Enum("async".to_owned()));
    assert_eq!(main.children.len(), 1);
    assert_eq!(
        main.children[0].fields["target"].value,
        Value::Reference("greet".to_owned())
    );
    let Value::List(parameters) = &main.fields["parameters"].value else {
        panic!("parameters are represented as typed records");
    };
    assert_eq!(parameters.len(), 4);
    let Value::Properties(first) = &parameters[0].value else {
        panic!("parameter is a property record");
    };
    assert_eq!(first["name"].value, Value::Identifier("first".to_owned()));
    assert_eq!(first["annotation"].value, Value::Text("str".to_owned()));
    assert_eq!(
        main.fields["return_type"].value,
        Value::Text("str".to_owned())
    );

    let typed = generated::Module::try_from(root)
        .expect("generated typed AST accepts CPython-backed nodes");
    assert_eq!(typed.children.len(), 2);
}

#[test]
fn cpython_ast_columns_and_syntax_errors_use_utf8_byte_spans() {
    let source = SourceFile::new(
        "unicode.py",
        "# 🟦 before declarations\ndef target(value: str) -> str:\n    return value\n\ndef caller() -> str:\n    return target('é')\n",
    );
    let analysis = analyze_source(&source);
    assert!(!has_errors(&analysis), "{:?}", analysis.diagnostics);
    let call = &function(&analysis.parsed.program.nodes[0], "caller").children[0];
    let span = call.fields["target"].span;
    assert_eq!(&source.text[span.start..span.end], "target");
    assert_eq!(&source.text[call.span.start..call.span.end], "target('é')");

    let malformed = SourceFile::new(
        "broken.py",
        "# 🟦 before error\ndef broken(value: ) -> str:\n    return value\n",
    );
    let broken = analyze_source(&malformed);
    let diagnostic = broken
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "PS1001")
        .expect("CPython syntax diagnostic is preserved");
    assert_eq!(diagnostic.severity, Severity::Error);
    let span = diagnostic
        .span
        .expect("syntax diagnostic has a source span");
    assert!(span.start <= span.end && span.end <= malformed.text.len());
    assert!(malformed.text.is_char_boundary(span.start));
    assert!(malformed.text.is_char_boundary(span.end));
}

#[test]
fn valid_function_siblings_are_recovered_around_a_syntax_error() {
    let source = SourceFile::new(
        "recovery.py",
        "from .helpers import target\ndef before() -> str:\n    return target()\n\ndef broken( -> str:\n    return 'bad'\n\ndef after() -> str:\n    return target()\n",
    );
    let analysis = analyze_source(&source);
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "PS1001"),
        "{:?}",
        analysis.diagnostics
    );
    let root = analysis
        .parsed
        .program
        .nodes
        .first()
        .expect("syntax recovery returns a module node");
    assert!(
        root.children
            .iter()
            .any(|node| node.construct == "python_import")
    );
    assert!(root.children.iter().any(|node| {
        matches!(node.fields.get("name").map(|field| &field.value),
            Some(Value::Identifier(name)) if name == "before")
    }));
    assert!(root.children.iter().any(|node| {
        matches!(node.fields.get("name").map(|field| &field.value),
            Some(Value::Identifier(name)) if name == "after")
    }));
}

#[test]
fn indexing_python_bodies_does_not_execute_them() {
    let source = SourceFile::new(
        "not-executed.py",
        "def dangerous() -> None:\n    __import__('pathlib').Path('/tmp/langmeld-python-source-executed').touch()\n",
    );
    let analysis = analyze_source(&source);
    assert!(!has_errors(&analysis), "{:?}", analysis.diagnostics);
    assert_eq!(analysis.parsed.program.nodes[0].children.len(), 1);
}

#[test]
fn relative_import_aliases_resolve_across_python_modules() {
    let main = SourceFile::new(
        "src/main.py",
        "from .greetings import greet as say_hello, greet_alt\ndef main() -> str:\n    return say_hello() + greet_alt()\n",
    );
    let greetings = SourceFile::new(
        "src/greetings.py",
        "def greet() -> str:\n    return 'hello'\ndef greet_alt() -> str:\n    return 'welcome'\n",
    );
    let analysis = analyze_sources(&[main.clone(), greetings.clone()]);
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let references = analysis
        .resolved_references
        .iter()
        .filter(|reference| reference.source_index == 0)
        .collect::<Vec<_>>();
    assert_eq!(references.len(), 2);
    for reference in references {
        let spelling = &main.text[reference.reference_span.start..reference.reference_span.end];
        assert!(matches!(spelling, "say_hello" | "greet_alt"));
        assert_eq!(reference.declaration_source_index, 1);
        assert!(matches!(
            &greetings.text[reference.declaration_span.start..reference.declaration_span.end],
            "greet" | "greet_alt"
        ));
    }
}

#[test]
fn same_named_functions_remain_scoped_to_their_python_modules() {
    let first = SourceFile::new(
        "src/first.py",
        "def greet() -> str:\n    return 'first'\ndef caller() -> str:\n    return greet()\n",
    );
    let second = SourceFile::new(
        "src/second.py",
        "def greet() -> str:\n    return 'second'\ndef caller() -> str:\n    return greet()\n",
    );
    let analysis = analyze_sources(&[first, second]);
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    assert_eq!(analysis.resolved_references.len(), 2);
    assert_ne!(
        analysis.resolved_references[0].declaration_source_index,
        analysis.resolved_references[1].declaration_source_index
    );
}

#[test]
fn missing_relative_modules_and_functions_are_reported_at_imports() {
    let main = SourceFile::new(
        "src/main.py",
        "from .missing import absent\ndef main() -> str:\n    return absent()\n",
    );
    let analysis = analyze_sources(std::slice::from_ref(&main));
    assert!(
        analysis.diagnostics.iter().any(|diagnostic| {
            diagnostic.source_index == 0 && diagnostic.diagnostic.code == "PS1004"
        }),
        "{:?}",
        analysis.diagnostics
    );

    let main = SourceFile::new(
        "src/main.py",
        "from .helpers import absent\ndef main() -> str:\n    return absent()\n",
    );
    let helpers = SourceFile::new("src/helpers.py", "def private() -> str:\n    return 'x'\n");
    let analysis = analyze_sources(&[main, helpers]);
    assert!(analysis.diagnostics.iter().any(|diagnostic| {
        diagnostic.source_index == 0 && diagnostic.diagnostic.code == "PS1005"
    }));
}

#[test]
fn project_manifest_selects_python_files_and_indexes_package_initializers_as_modules() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("project/langmeld.json");
    let project = load_project(&manifest).expect("Python fixture manifest loads");
    assert_eq!(project.sources.len(), 5);
    assert!(
        project
            .sources
            .iter()
            .any(|source| source.name.ends_with("/__init__.py"))
    );
    assert!(
        project
            .sources
            .iter()
            .all(|source| source.name.ends_with(".py"))
    );
}

#[test]
fn opaque_python_regions_do_not_claim_generic_identifier_highlighting() {
    let source = SourceFile::new("main.py", "def greet(name: str) -> str:\n    return name\n");
    let parser = configured_parser().expect("Python parser configures");
    let parsed = parser.parse(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert!(classify_parse_result_prepared(parser.language(), &source, &parsed).is_empty());
}
