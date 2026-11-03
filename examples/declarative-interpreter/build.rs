use std::{env, fs, path::PathBuf};

fn main() {
    if let Err(error) = generate_language_model() {
        panic!("cannot compile Langmeld language model: {error}");
    }
}

fn generate_language_model() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = root.join("langmeld.json");
    let manifest = langmeld::language::load_project_manifest(&manifest_path)?;
    let language = langmeld::language::load_project_language(&manifest_path)?;
    println!("cargo:rerun-if-changed=langmeld.json");
    println!("cargo:rerun-if-changed=langmeld.lock");
    for path in langmeld::language::project_definition_source_paths_for_manifest(
        &manifest_path,
        &manifest,
        &language,
    )? {
        let relative = path
            .strip_prefix(&root)
            .map_err(|_| format!("definition path '{}' escaped the project", path.display()))?;
        println!("cargo:rerun-if-changed={}", relative.display());
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?)
        .join("langmeld_language.rs");
    fs::write(
        output,
        langmeld::language::generate_rust_language_spec(&language),
    )
    .map_err(|error| format!("cannot write compiled language model: {error}"))
}
