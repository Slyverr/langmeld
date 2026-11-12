use std::{io, path::Path, path::PathBuf};

use super::Template;

const WORKSPACE_CRATES: &[&str] = &[
    "langmeld-core",
    "langmeld-language",
    "langmeld-project",
    "langmeld-lsp",
    "langmeld-build",
];
const DECLARATIVE_CRATES: &[&str] = &["langmeld-core", "langmeld-language", "langmeld-project"];
const TYPED_DECLARATIVE_CRATES: &[&str] = &[
    "langmeld-core",
    "langmeld-language",
    "langmeld-project",
    "langmeld-build",
];
const CUSTOM_PARSER_CRATES: &[&str] = &["langmeld-core", "langmeld-language", "langmeld-lsp"];

pub(super) fn workspace_root() -> Option<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|path| is_langmeld_workspace(path))
        .and_then(|path| path.canonicalize().ok())
}

pub(super) fn cargo_config(
    workspace_root: &Path,
    template: Template,
    typed_bindings: bool,
) -> io::Result<String> {
    let workspace_root = workspace_root.canonicalize()?;
    let crate_names = match template {
        Template::Declarative if typed_bindings => TYPED_DECLARATIVE_CRATES,
        Template::Declarative => DECLARATIVE_CRATES,
        Template::CustomParser => CUSTOM_PARSER_CRATES,
    };
    let mut config = String::from("[patch.crates-io]\n");

    for crate_name in crate_names {
        let crate_path = workspace_root.join("crates").join(crate_name);
        if !crate_path.join("Cargo.toml").is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Langmeld source checkout is missing `{crate_name}`"),
            ));
        }
        let crate_path = crate_path.to_str().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Langmeld source path cannot be represented as UTF-8",
            )
        })?;
        config.push_str(crate_name);
        config.push_str(" = { path = ");
        config.push_str(&quote_toml_string(crate_path));
        config.push_str(" }\n");
    }

    Ok(config)
}

fn is_langmeld_workspace(path: &Path) -> bool {
    path.join("Cargo.toml").is_file()
        && WORKSPACE_CRATES.iter().all(|crate_name| {
            path.join("crates")
                .join(crate_name)
                .join("Cargo.toml")
                .is_file()
        })
}

fn quote_toml_string(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');

    for character in value.chars() {
        match character {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\u{0008}' => quoted.push_str("\\b"),
            '\t' => quoted.push_str("\\t"),
            '\n' => quoted.push_str("\\n"),
            '\u{000C}' => quoted.push_str("\\f"),
            '\r' => quoted.push_str("\\r"),
            character if character.is_control() => {
                quoted.push_str(&format!("\\u{:04X}", u32::from(character)));
            }
            character => quoted.push(character),
        }
    }

    quoted.push('"');
    quoted
}
