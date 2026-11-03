use functions_language::{
    RuntimeContext, analyze_sources, generate_rust, load_language, load_language_runtime,
    load_project, load_project_runtime, parse_source,
};
use langmeld::core::SourceFile;
use langmeld::language::generate_tree_sitter;
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn rust_authored_language_parses_and_lowers_function_signatures() {
    let language = load_language().expect("compiled language model loads");
    assert_eq!(language.spec().name, "Functions");
    let runtime_language = load_language_runtime().expect("runtime language model loads");
    assert_eq!(language.spec(), runtime_language.spec());
    let compiled_project = load_project().expect("compiled project model loads");
    let runtime_project = load_project_runtime().expect("runtime project model loads");
    assert_eq!(
        compiled_project.language.spec(),
        runtime_project.language.spec()
    );

    let source = SourceFile::new(
        "hello.functions.lmprog",
        include_str!("../examples/hello.functions.lmprog"),
    );
    let parsed = parse_source(&source).expect("parser prepares");
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let root = &parsed.program.nodes[0];
    assert_eq!(root.construct, "module");
    assert_eq!(root.children.len(), 3);
    assert!(root.children.iter().all(|child| child.construct == "function"));

    let sources = vec![source];
    let programs = vec![parsed.program];
    let analysis = analyze_sources(&language, &sources, &[&programs[0]])
        .expect("generic workspace analysis runs");
    assert!(
        analysis.diagnostics.is_empty(),
        "{:#?}",
        analysis.diagnostics
    );
    let rust = generate_rust(&RuntimeContext {
        language: &language,
        sources: &sources,
        programs: &programs,
        analysis: &analysis,
    })
    .expect("host backend lowers signatures");
    assert_eq!(
        rust,
        "pub fn empty() -> i32 {\n    0\n}\n\npub fn identity(value: i32) -> i32 {\n    value\n}\n\npub unsafe fn add(left: i32, right: i32, extra: i32) -> i32 {\n    left + right + extra\n}\n\n"
    );

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "langmeld-functions-output-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).expect("temporary compiler directory is created");
    let source_path = directory.join("generated.rs");
    fs::write(&source_path, rust).expect("generated Rust source is written");
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let compiled = Command::new(rustc)
        .args(["--edition=2024", "--crate-type=lib", "--emit=metadata"])
        .arg("--out-dir")
        .arg(&directory)
        .arg(&source_path)
        .output()
        .expect("Rust compiler starts");
    let _ = fs::remove_dir_all(&directory);
    assert!(
        compiled.status.success(),
        "generated Rust did not compile:\n{}\n{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
}

#[test]
fn lowering_rejects_types_outside_its_supported_target_contract() {
    let language = load_language().expect("compiled language model loads");
    let source = SourceFile::new(
        "unsupported.functions.lmprog",
        "module {\n  fn empty() -> String\n}\n",
    );
    let parsed = parse_source(&source).expect("parser prepares");
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let sources = vec![source];
    let programs = vec![parsed.program];
    let analysis = analyze_sources(&language, &sources, &[&programs[0]])
        .expect("generic workspace analysis runs");
    let error = generate_rust(&RuntimeContext {
        language: &language,
        sources: &sources,
        programs: &programs,
        analysis: &analysis,
    })
    .expect_err("unsupported target types must fail explicitly");
    assert!(error.contains("unsupported type `String`"));
}

#[test]
#[ignore = "emits a Tree-sitter fixture from the Rust-authored language model"]
fn write_rust_authored_tree_sitter_validation_fixture() {
    let output = std::env::var_os("LANGMELD_RUST_TREE_SITTER_FIXTURE_DIR")
        .expect("fixture output directory is configured");
    let output = std::path::PathBuf::from(output);
    fs::create_dir_all(&output).expect("fixture directory is created");
    for (path, contents) in generate_tree_sitter(&load_language().expect("language loads"))
        .expect("Rust-authored grammar exports")
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
    let invalid = output.join("invalid/one-parameter.functions.lmprog");
    fs::create_dir_all(invalid.parent().expect("invalid fixture has a parent"))
        .expect("invalid fixture directory is created");
    fs::write(
        invalid,
        "module {\n  fn invalid(value) -> i32\n}\n",
    )
    .expect("invalid function signature is written");
}
