use langmeld::core::{Severity, SourceFile};
use langmeld::language::{AstNode, Value, classify_parse_result_prepared};
use typescript_source_language::{
    analyze_source, analyze_sources, configured_parser, generated, has_errors,
};

fn function<'a>(root: &'a AstNode, name: &str) -> &'a AstNode {
    root.children
        .iter()
        .find(|function| {
            matches!(
                function.fields.get("name").map(|field| &field.value),
                Some(Value::Identifier(actual)) if actual == name
            )
        })
        .unwrap_or_else(|| panic!("TypeScript function `{name}` is indexed"))
}

#[test]
fn typescript_ast_maps_functions_parameters_async_annotations_and_calls() {
    let source = SourceFile::new("main.ts", include_str!("../main.ts"));
    let analysis = analyze_source(&source);
    assert!(!has_errors(&analysis), "{:?}", analysis.diagnostics);
    let root = &analysis.parsed.program.nodes[0];
    assert_eq!(root.children.len(), 2);

    let format = function(root, "formatGreeting");
    let Value::List(parameters) = &format.fields["parameters"].value else {
        panic!("TypeScript parameters are mapped as a record list");
    };
    assert_eq!(parameters.len(), 2);
    let Value::Properties(first) = &parameters[0].value else {
        panic!("parameter is mapped as a typed property record");
    };
    assert_eq!(first["name"].value, Value::Text("name".to_owned()));
    assert_eq!(first["annotation"].value, Value::Text("string".to_owned()));
    assert_eq!(
        format.fields["return_type"].value,
        Value::Text("string".to_owned())
    );

    let main = function(root, "main");
    assert_eq!(main.fields["kind"].value, Value::Enum("async".to_owned()));
    assert_eq!(main.children.len(), 1);
    let call = &main.children[0];
    assert_eq!(
        call.fields["target"].value,
        Value::Reference("formatGreeting".to_owned())
    );
    assert_eq!(
        &source.text[call.fields["target"].span.start..call.fields["target"].span.end],
        "formatGreeting"
    );
    assert_eq!(
        &source.text[call.span.start..call.span.end],
        "formatGreeting(name, \"Welcome\")"
    );

    let typed = generated::Module::try_from(root)
        .expect("generated typed API accepts the TypeScript compiler tree");
    assert_eq!(typed.children.len(), 2);
}

#[test]
fn typescript_compiler_offsets_are_translated_to_utf8_bytes() {
    let source = SourceFile::new(
        "unicode.ts",
        "// 🟦 TypeScript source\nfunction target(value: string): string { return value; }\nfunction caller(): string { return target('é'); }\n",
    );
    let analysis = analyze_source(&source);
    assert!(!has_errors(&analysis), "{:?}", analysis.diagnostics);
    let call = &function(&analysis.parsed.program.nodes[0], "caller").children[0];
    let target = &call.fields["target"];
    assert_eq!(&source.text[target.span.start..target.span.end], "target");
    assert_eq!(&source.text[call.span.start..call.span.end], "target('é')");
}

#[test]
fn opaque_typescript_regions_do_not_claim_generic_identifier_highlighting() {
    let source = SourceFile::new("main.ts", include_str!("../main.ts"));
    let parser = configured_parser().expect("TypeScript parser configures");
    let parsed = parser.parse(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert!(classify_parse_result_prepared(parser.language(), &source, &parsed).is_empty());
}

#[test]
fn typescript_syntax_diagnostics_use_utf8_source_spans() {
    let source = SourceFile::new(
        "broken.ts",
        "// 🟦 before malformed syntax\nfunction broken(value: ): string { return value; }\n",
    );
    let analysis = analyze_source(&source);
    let diagnostic = analysis
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "TS1001")
        .expect("TypeScript parse diagnostic is reported");
    let span = diagnostic.span.expect("parse error has a source span");
    assert_eq!(diagnostic.severity, Severity::Error);
    assert!(span.start <= span.end && span.end <= source.text.len());
    assert!(source.text.is_char_boundary(span.start));
    assert!(source.text.is_char_boundary(span.end));
}

#[test]
fn typescript_parser_keeps_valid_functions_when_later_syntax_is_incomplete() {
    let source = SourceFile::new(
        "incomplete.ts",
        "function target(): string { return 'ready'; }\nfunction caller(): string { return target(); }\nfunction unfinished(value: string): string {\n  return target(\n",
    );
    let analysis = analyze_source(&source);
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TS1001")
    );
    let root = analysis
        .parsed
        .program
        .nodes
        .first()
        .expect("partial TypeScript syntax still returns a module node");
    assert!(root.children.iter().any(|node| {
        matches!(
            node.fields.get("name").map(|field| &field.value),
            Some(Value::Identifier(name)) if name == "caller"
        )
    }));
}

#[test]
fn source_index_parses_but_never_executes_function_bodies() {
    let source = SourceFile::new(
        "not-executed.ts",
        "function dangerous(): never { throw new Error('indexing must not execute source'); }\n",
    );
    let analysis = analyze_source(&source);
    assert!(!has_errors(&analysis), "{:?}", analysis.diagnostics);
    assert_eq!(analysis.parsed.program.nodes[0].children.len(), 1);
}

#[test]
fn named_import_calls_resolve_across_typescript_project_files() {
    let dependency = SourceFile::new(
        "src/greetings.ts",
        "export function formatGreeting(name: string): string { return name; }\n",
    );
    let consumer = SourceFile::new(
        "src/main.ts",
        "import { formatGreeting as greet } from './greetings';\nimport { formatGreeting as alternate } from './other';\nimport { formatGreeting as fromIndex } from './shared';\nfunction main(): string { return `${greet('Ada')} ${alternate('Grace')} ${fromIndex('Lin')}`; }\n",
    );
    let other = SourceFile::new(
        "src/other.ts",
        "export function formatGreeting(name: string): string { return `Hello, ${name}.`; }\n",
    );
    let index = SourceFile::new(
        "src/shared/index.ts",
        "export function formatGreeting(name: string): string { return `Welcome, ${name}.`; }\n",
    );
    let analysis = analyze_sources(&[
        consumer.clone(),
        dependency.clone(),
        other.clone(),
        index.clone(),
    ]);
    assert!(
        analysis.diagnostics.is_empty(),
        "project diagnostics: {:?}",
        analysis.diagnostics
    );
    let references = analysis
        .resolved_references
        .iter()
        .filter(|reference| reference.source_index == 0)
        .collect::<Vec<_>>();
    assert_eq!(references.len(), 3);
    let greet = references
        .iter()
        .find(|reference| {
            &consumer.text[reference.reference_span.start..reference.reference_span.end] == "greet"
        })
        .expect("first imported call resolves through its module");
    assert_eq!(greet.declaration_source_index, 1);
    assert_eq!(
        &dependency.text[greet.declaration_span.start..greet.declaration_span.end],
        "formatGreeting"
    );
    let alternate = references
        .iter()
        .find(|reference| {
            &consumer.text[reference.reference_span.start..reference.reference_span.end]
                == "alternate"
        })
        .expect("second imported call resolves through its module");
    assert_eq!(alternate.declaration_source_index, 2);
    assert_eq!(
        &other.text[alternate.declaration_span.start..alternate.declaration_span.end],
        "formatGreeting"
    );
    let from_index = references
        .iter()
        .find(|reference| {
            &consumer.text[reference.reference_span.start..reference.reference_span.end]
                == "fromIndex"
        })
        .expect("directory imports resolve to index.ts scopes");
    assert_eq!(from_index.declaration_source_index, 3);
    assert_eq!(
        &index.text[from_index.declaration_span.start..from_index.declaration_span.end],
        "formatGreeting"
    );
}

#[test]
fn project_index_reports_missing_import_files_and_non_exported_functions() {
    let source = SourceFile::new(
        "src/main.ts",
        "import { hidden } from './definitions';\nfunction main(): string { return hidden(); }\n",
    );
    let definitions = SourceFile::new(
        "src/definitions.ts",
        "function hidden(): string { return 'private'; }\n",
    );
    let analysis = analyze_sources(&[source, definitions]);
    assert!(analysis.diagnostics.iter().any(|diagnostic| {
        diagnostic.source_index == 0 && diagnostic.diagnostic.code == "TS1005"
    }));

    let default_import = SourceFile::new(
        "src/main.ts",
        "import greet from './definitions';\nfunction main(): string { return greet(); }\n",
    );
    let definitions = SourceFile::new(
        "src/definitions.ts",
        "export function formatGreeting(): string { return 'private'; }\n",
    );
    let analysis = analyze_sources(&[default_import, definitions]);
    assert!(analysis.diagnostics.iter().any(|diagnostic| {
        diagnostic.source_index == 0 && diagnostic.diagnostic.code == "TS1005"
    }));

    let missing = SourceFile::new(
        "src/main.ts",
        "import { absent } from './missing';\nfunction main(): string { return absent(); }\n",
    );
    let analysis = analyze_sources(&[missing]);
    assert!(analysis.diagnostics.iter().any(|diagnostic| {
        diagnostic.source_index == 0 && diagnostic.diagnostic.code == "TS1004"
    }));
}
