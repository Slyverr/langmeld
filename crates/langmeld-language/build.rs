use std::{
    env,
    fmt::Write as _,
    fs, io,
    path::{Component, Path, PathBuf},
};

fn main() {
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR"));
    let source_dir = manifest_dir.join("src/definition_meta");
    let root = source_dir.join("language.lm");
    let modules_dir = source_dir.join("constructs");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"))
        .join("definition_meta_sources.rs");

    println!("cargo:rerun-if-changed={}", root.display());

    let mut modules = Vec::new();
    discover_modules(&modules_dir, &mut modules).unwrap_or_else(|error| {
        panic!(
            "cannot discover embedded definition modules in {}: {error}",
            modules_dir.display()
        )
    });
    modules.sort();

    let mut generated = String::from("pub const ROOT: &str = include_str!(");
    write_rust_path(&mut generated, &root);
    generated.push_str(");\npub const MODULES: &[(&str, &str)] = &[\n");
    for path in modules {
        println!("cargo:rerun-if-changed={}", path.display());
        let relative = path
            .strip_prefix(&source_dir)
            .expect("definition modules are under the source directory");
        let virtual_path = relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(part) => Some(part.to_string_lossy()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        writeln!(
            generated,
            "    ({:?}, include_str!({:?})),",
            format!("language/{virtual_path}"),
            path.to_string_lossy()
        )
        .expect("writing generated Rust source cannot fail");
    }
    generated.push_str("];\n");

    fs::write(&output, generated).unwrap_or_else(|error| {
        panic!(
            "cannot write embedded definition source registry {}: {error}",
            output.display()
        )
    });
}

fn write_rust_path(output: &mut String, path: &Path) {
    write!(output, "{:?}", path.to_string_lossy())
        .expect("writing a Rust string literal cannot fail");
}

fn discover_modules(directory: &Path, modules: &mut Vec<PathBuf>) -> io::Result<()> {
    println!("cargo:rerun-if-changed={}", directory.display());
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            discover_modules(&path, modules)?;
        } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "lm") {
            modules.push(path);
        }
    }
    Ok(())
}
