use std::{env, fs, path::PathBuf};

#[path = "src/language/spec.rs"]
mod language_spec;

fn main() {
    if let Err(error) = generate_language_model() {
        panic!("cannot compile Rust source language model: {error}");
    }
}

fn generate_language_model() -> Result<(), String> {
    let language = language_spec::validated_language().map_err(|errors| {
        errors
            .into_iter()
            .map(|error| format!("{}: {}", error.code, error.message))
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let output_dir = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    println!("cargo:rerun-if-changed=src/language/spec.rs");
    fs::write(
        output_dir.join("langmeld_language.rs"),
        langmeld::language::generate_rust_language_spec(&language),
    )
    .map_err(|error| format!("cannot write compiled language model: {error}"))?;
    fs::write(
        output_dir.join("langmeld_ast.rs"),
        langmeld::language::generate_rust_ast(&language),
    )
    .map_err(|error| format!("cannot write typed Rust source AST: {error}"))
}
