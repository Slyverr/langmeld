use csignatures_language::{
    RuntimeContext, analyze_sources, generate_rust, load_language, load_language_runtime,
    load_project, load_project_runtime, parse_source,
};
use langmeld::core::SourceFile;
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn custom_c_parser_lowers_prototypes_to_compilable_rust_ffi() {
    let language = load_language().expect("compiled language model loads");
    assert_eq!(language.spec().name, "CSignatures");
    let runtime_language = load_language_runtime().expect("runtime language model loads");
    assert_eq!(language.spec(), runtime_language.spec());
    let compiled_project = load_project().expect("compiled project model loads");
    let runtime_project = load_project_runtime().expect("runtime project model loads");
    assert_eq!(
        compiled_project.language.spec(),
        runtime_project.language.spec()
    );

    let source = SourceFile::new("math.c", include_str!("../examples/math.c"));
    let parsed = parse_source(&source).expect("custom parser prepares");
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(parsed.custom_parser_spans.len(), 1);
    let translation_unit = &parsed.program.nodes[0];
    assert_eq!(translation_unit.construct, "translation_unit");
    assert_eq!(translation_unit.children.len(), 2);
    assert_eq!(translation_unit.children[0].construct, "prototype0");
    assert_eq!(translation_unit.children[1].construct, "prototype2");

    let sources = vec![source];
    let programs = vec![parsed.program];
    let analysis = analyze_sources(&language, &sources, &[&programs[0]])
        .expect("generic analysis runs on custom syntax nodes");
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
    .expect("host lowers C prototypes to Rust FFI declarations");
    assert_eq!(
        rust,
        "unsafe extern \"C\" {\n    pub fn reset();\n    pub fn add(left: i32, right: i32) -> i32;\n}\n"
    );

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "langmeld-c-signatures-output-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).expect("temporary compiler directory is created");
    let source_path = directory.join("generated.rs");
    fs::write(&source_path, rust).expect("generated Rust bindings are written");
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
        "generated Rust bindings did not compile:\n{}\n{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
}

#[test]
fn custom_c_parser_reports_unsupported_types() {
    let source = SourceFile::new("unsupported.c", "long count(void);\n");
    let parsed = parse_source(&source).expect("custom parser prepares");
    assert!(parsed.program.nodes.is_empty());
    assert_eq!(parsed.diagnostics.len(), 1);
    assert_eq!(parsed.diagnostics[0].code, "LMCS1001");
    assert!(
        parsed.diagnostics[0]
            .message
            .contains("unsupported C return type")
    );
}
