use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
};

pub(crate) const MODULES_BEGIN: &str = "// <langmeld:begin modules>";
pub(crate) const MODULES_END: &str = "// <langmeld:end modules>";
pub(crate) const REGISTRATIONS_BEGIN: &str = "// <langmeld:begin registrations>";
pub(crate) const REGISTRATIONS_END: &str = "// <langmeld:end registrations>";

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

    let line_ending = if region.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let updated_region = format!("{}{line_ending}", lines.join(line_ending));
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

pub(crate) fn handler_template(
    construct: &str,
    handler_name: &str,
    typed_name: Option<&str>,
) -> String {
    let construct_declaration = construct_declaration(construct, typed_name);
    let typed_access = typed_name.map_or_else(String::new, |type_name| {
        format!(
            "        let _syntax = context.typed::<crate::generated::constructs::{type_name}>()?;\n\n"
        )
    });

    format!(
        r#"use langmeld_language::{{ConstructContext, ConstructHandler, HandlerControl}};

use crate::runtime::RuntimeState;

pub(super) struct {handler_name};

impl ConstructHandler<RuntimeState> for {handler_name} {{
    {construct_declaration}

    fn handle(
        &self,
        context: &mut ConstructContext<'_, RuntimeState>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {{
{typed_access}        Err(context.error_at(
            "LM-HANDLER-005",
            "implement this construct's language behavior",
            context.construct().span,
        ))
    }}
}}
"#
    )
}

pub(crate) fn construct_declaration(construct: &str, typed_name: Option<&str>) -> String {
    typed_name.map_or_else(
        || format!("const CONSTRUCT: &'static str = {construct:?};"),
        |type_name| {
            format!(
                "const CONSTRUCT: &'static str =\n        <crate::generated::constructs::{type_name}<'static> as langmeld_language::TypedConstruct<\n            'static,\n        >>::CONSTRUCT;"
            )
        },
    )
}
