use langmeld::core::SourceFile;
use std::{env, fs, path::PathBuf};

fn main() {
    if let Err(error) = generate_ast() {
        panic!("cannot compile TypeScript source model: {error}");
    }
}

fn generate_ast() -> Result<(), String> {
    let definition_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("typescript-source.lm");
    let definition = fs::read_to_string(&definition_path)
        .map_err(|error| format!("cannot read '{}': {error}", definition_path.display()))?;
    println!("cargo:rerun-if-changed={}", definition_path.display());
    let language = langmeld::language::parse_definition(&SourceFile::new(
        definition_path.display().to_string(),
        definition,
    ))
    .map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
            .collect::<Vec<_>>()
            .join("\n")
    })?
    .validate()
    .map_err(|errors| {
        errors
            .into_iter()
            .map(|error| format!("{}: {}", error.code, error.message))
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let output =
        PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?).join("langmeld_ast.rs");
    fs::write(output, langmeld::language::generate_rust_ast(&language))
        .map_err(|error| format!("cannot write typed TypeScript source AST: {error}"))
}
