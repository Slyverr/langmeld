use std::{fs, io, path::Path};

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

pub(crate) fn ensure_new_path(path: &Path, description: &str) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{description} `{}` already exists", path.display()),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub(crate) fn find_handler_module_collision<'a>(
    construct: &str,
    module_name: &str,
    declared_constructs: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    declared_constructs
        .into_iter()
        .filter(|candidate| *candidate != construct)
        .find(|candidate| {
            rust_names(candidate).is_ok_and(|(candidate_module, _)| candidate_module == module_name)
        })
        .map(str::to_owned)
}

pub(crate) fn ensure_new_handler_path(
    path: &Path,
    construct: &str,
    module_name: &str,
    colliding_construct: Option<&str>,
) -> io::Result<()> {
    ensure_new_path(path, "handler source").map_err(|error| {
        if error.kind() != io::ErrorKind::AlreadyExists {
            return error;
        }

        let Some(colliding_construct) = colliding_construct else {
            return error;
        };

        let collision_error =
            handler_module_collision_error(construct, colliding_construct, module_name);
        io::Error::new(error.kind(), format!("{error}; {collision_error}"))
    })
}

pub(crate) fn handler_module_collision_error(
    construct: &str,
    colliding_construct: &str,
    module_name: &str,
) -> io::Error {
    io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "constructs `{construct}` and `{colliding_construct}` both map to Rust module `{module_name}`; rename one construct to generate separate handlers"
        ),
    )
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

pub(crate) fn update_handler_registry(
    source: &str,
    module_line: &str,
    registration_line: &str,
) -> Result<String, String> {
    let source = update_managed_section(source, MODULES_BEGIN, MODULES_END, module_line)?;

    update_managed_section(
        &source,
        REGISTRATIONS_BEGIN,
        REGISTRATIONS_END,
        registration_line,
    )
}

pub(crate) fn rust_names(construct: &str) -> Result<(String, String), io::Error> {
    let names = langmeld_build::construct_handler_names(construct)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;

    Ok((names.module_name, names.handler_name))
}

pub(crate) fn handler_template(
    construct: &str,
    handler_name: &str,
    typed_name: Option<&str>,
) -> String {
    let construct_declaration = construct_declaration(construct, typed_name);
    let handler_trait = handler_trait_name(typed_name);
    let handler_imports = handler_imports(typed_name);
    let handle_signature = handler_signature(typed_name, "RuntimeState", "_syntax");

    format!(
        r#"use langmeld_language::{handler_imports};

use crate::runtime::RuntimeState;

pub(super) struct {handler_name};

impl {handler_trait}<RuntimeState> for {handler_name} {{
    {construct_declaration}

    {handle_signature}
        Err(context.error_at(
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
            format!("type View<'source> = crate::generated::constructs::{type_name}<'source>;")
        },
    )
}

pub(crate) fn registration_method(typed_name: Option<&str>) -> &'static str {
    if typed_name.is_some() {
        "register_typed"
    } else {
        "register"
    }
}

pub(crate) fn handler_trait_name(typed_name: Option<&str>) -> &'static str {
    if typed_name.is_some() {
        "TypedConstructHandler"
    } else {
        "ConstructHandler"
    }
}

pub(crate) fn handler_imports(typed_name: Option<&str>) -> &'static str {
    if typed_name.is_some() {
        "{ConstructContext, HandlerControl, TypedConstructHandler}"
    } else {
        "{ConstructContext, ConstructHandler, HandlerControl}"
    }
}

pub(crate) fn handler_signature(
    typed_name: Option<&str>,
    state_type: &str,
    view_parameter: &str,
) -> String {
    if typed_name.is_some() {
        format!(
            "fn handle<'source>(\n        &self,\n        context: &mut ConstructContext<'source, {state_type}>,\n        {view_parameter}: Self::View<'source>,\n    ) -> Result<HandlerControl, langmeld_core::Diagnostic>\n    where\n        {state_type}: 'source,\n        (): 'source,\n    {{"
        )
    } else {
        format!(
            "fn handle(\n        &self,\n        context: &mut ConstructContext<'_, {state_type}>,\n    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {{"
        )
    }
}
