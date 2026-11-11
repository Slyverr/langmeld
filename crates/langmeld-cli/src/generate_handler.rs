use std::{
    error::Error,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use clap::Args;

use crate::project::load_definition_project;

pub(crate) const MODULES_BEGIN: &str = "// <langmeld:begin modules>";
pub(crate) const MODULES_END: &str = "// <langmeld:end modules>";
pub(crate) const REGISTRATIONS_BEGIN: &str = "// <langmeld:begin registrations>";
pub(crate) const REGISTRATIONS_END: &str = "// <langmeld:end registrations>";

#[derive(Debug, Args)]
pub(crate) struct HandlerArgs {
    /// Name of a construct declared by the language definition.
    construct: String,
    /// Path to langmeld.toml.
    #[arg(long, default_value = "langmeld.toml")]
    manifest_path: PathBuf,
}

pub(crate) fn generate_handler(args: HandlerArgs) -> Result<(), Box<dyn Error>> {
    let project = load_definition_project(&args.manifest_path)?;
    if !project
        .language
        .constructs()
        .iter()
        .any(|construct| construct.name() == args.construct)
    {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "construct `{}` is not declared in the language definition",
                args.construct
            ),
        )
        .into());
    }

    let (module_name, handler_name) = rust_names(&args.construct)?;
    validate_constructs_directory(&project.root.join("src"))?;
    let constructs_directory = project.root.join("src/constructs");
    validate_constructs_directory(&constructs_directory)?;

    let handler_path = constructs_directory.join(format!("{module_name}.rs"));
    if handler_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("handler source `{}` already exists", handler_path.display()),
        )
        .into());
    }

    let registry_path = constructs_directory.join("mod.rs");
    let registry_metadata = fs::symlink_metadata(&registry_path)?;
    if registry_metadata.file_type().is_symlink() || !registry_metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("refusing to update symlink `{}`", registry_path.display()),
        )
        .into());
    }
    let registry_source = fs::read_to_string(&registry_path)?;
    if registry_source.contains(&format!("::{handler_name})?;")) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "a handler for construct `{}` is already registered",
                args.construct
            ),
        )
        .into());
    }

    let module_line = format!("mod {module_name};");
    let registration_line = format!("handlers.register({module_name}::{handler_name})?;");
    let updated_registry =
        update_managed_section(&registry_source, MODULES_BEGIN, MODULES_END, &module_line)
            .and_then(|source| {
                update_managed_section(
                    &source,
                    REGISTRATIONS_BEGIN,
                    REGISTRATIONS_END,
                    &registration_line,
                )
            })
            .map_err(|message| io::Error::new(io::ErrorKind::InvalidData, message))?;
    let handler_source = handler_template(&args.construct, &handler_name);

    write_new_file(&handler_path, &handler_source)?;
    if let Err(error) = fs::write(&registry_path, updated_registry) {
        let restore_error = fs::write(&registry_path, registry_source).err();
        let cleanup_error = fs::remove_file(&handler_path).err();
        let detail = match (restore_error, cleanup_error) {
            (None, None) => error.to_string(),
            (restore, cleanup) => format!(
                "{error}; registry restore: {}; generated file cleanup: {}",
                restore.map_or_else(|| "succeeded".to_owned(), |error| error.to_string()),
                cleanup.map_or_else(|| "succeeded".to_owned(), |error| error.to_string()),
            ),
        };
        return Err(io::Error::other(detail).into());
    }

    println!(
        "Created `src/constructs/{module_name}.rs` and registered `{}`.",
        args.construct
    );

    Ok(())
}

pub(crate) fn validate_constructs_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("`{}` must be a regular directory", path.display()),
        ));
    }

    Ok(())
}

pub(crate) fn write_new_file(path: &Path, contents: &str) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    if let Err(error) = file.write_all(contents.as_bytes()) {
        drop(file);
        let cleanup_error = fs::remove_file(path).err();
        return Err(match cleanup_error {
            Some(cleanup_error) => io::Error::new(
                error.kind(),
                format!("{error}; could not remove incomplete file: {cleanup_error}"),
            ),
            None => error,
        });
    }

    Ok(())
}

pub(crate) fn update_managed_section(
    source: &str,
    begin_marker: &str,
    end_marker: &str,
    new_line: &str,
) -> Result<String, String> {
    let begin_positions = source.match_indices(begin_marker).collect::<Vec<_>>();
    let end_positions = source.match_indices(end_marker).collect::<Vec<_>>();
    if begin_positions.len() != 1 || end_positions.len() != 1 {
        return Err(format!(
            "expected exactly one `{begin_marker}` and `{end_marker}` marker pair"
        ));
    }

    let begin_end = begin_positions[0].0 + begin_marker.len();
    let region_start = source[begin_end..]
        .find('\n')
        .map(|offset| begin_end + offset + 1)
        .ok_or_else(|| format!("`{begin_marker}` must be on its own line"))?;
    let end_marker_position = end_positions[0].0;
    if end_marker_position < region_start {
        return Err(format!("`{end_marker}` appears before `{begin_marker}`"));
    }
    let region_end = source[..end_marker_position]
        .rfind('\n')
        .map_or(0, |offset| offset + 1);
    if source[region_end..end_marker_position]
        .chars()
        .any(|character| !character.is_whitespace())
    {
        return Err(format!("`{end_marker}` must be on its own line"));
    }

    let region = &source[region_start..region_end];
    let Some(existing_line) = region.lines().find(|line| !line.trim().is_empty()) else {
        return Err(format!("managed section `{begin_marker}` is empty"));
    };
    let indentation = existing_line
        .chars()
        .take_while(|character| character.is_whitespace())
        .collect::<String>();
    if region.lines().any(|line| line.trim() == new_line) {
        return Err(format!("`{new_line}` is already registered"));
    }

    let mut lines = region
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    lines.push(format!("{indentation}{new_line}"));
    lines.sort_by(|left, right| left.trim().cmp(right.trim()));

    let updated_region = format!("{}\n", lines.join("\n"));
    let mut updated = source.to_owned();
    updated.replace_range(region_start..region_end, &updated_region);

    Ok(updated)
}

pub(crate) fn rust_names(construct: &str) -> Result<(String, String), io::Error> {
    let mut words = Vec::new();
    let mut word = String::new();

    for character in construct.chars() {
        if character.is_ascii_alphanumeric() {
            word.push(character.to_ascii_lowercase());
        } else if matches!(character, '-' | '_') {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "construct `{construct}` cannot map to a Rust module name; use ASCII letters, digits, `-`, or `_`"
                ),
            ));
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    if words.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "construct name must contain an ASCII letter or digit",
        ));
    }

    let mut module_name = words.join("_");
    if module_name.starts_with(|character: char| character.is_ascii_digit()) {
        module_name.insert_str(0, "construct_");
    }
    if is_rust_keyword(&module_name) {
        module_name.push_str("_handler");
    }

    let mut handler_name = words
        .iter()
        .map(|word| {
            let mut characters = word.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + characters.as_str()
            })
        })
        .collect::<String>()
        + "Handler";
    if handler_name.starts_with(|character: char| character.is_ascii_digit()) {
        handler_name.insert_str(0, "Construct");
    }

    Ok((module_name, handler_name))
}

fn is_rust_keyword(identifier: &str) -> bool {
    matches!(
        identifier,
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "do"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "final"
            | "fn"
            | "for"
            | "gen"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "macro"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "override"
            | "priv"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "typeof"
            | "unsafe"
            | "unsized"
            | "use"
            | "virtual"
            | "where"
            | "while"
            | "yield"
    )
}

pub(crate) fn handler_template(construct: &str, handler_name: &str) -> String {
    format!(
        "use langmeld_language::{{ConstructContext, ConstructHandler, HandlerControl}};\n\nuse crate::runtime::RuntimeState;\n\npub(super) struct {handler_name};\n\nimpl ConstructHandler<RuntimeState> for {handler_name} {{\n    const CONSTRUCT: &'static str = {construct:?};\n\n    fn handle(\n        &self,\n        context: &mut ConstructContext<'_, RuntimeState>,\n    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {{\n        Err(context.error_at(\n            \"LM-HANDLER-005\",\n            \"implement this construct's language behavior\",\n            context.construct().span,\n        ))\n    }}\n}}\n"
    )
}
