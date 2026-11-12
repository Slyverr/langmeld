use std::{error::Error, fs, io, path::PathBuf};

use clap::Args;
use langmeld_project::RustBindings;

use crate::{
    atomic_file::replace_existing,
    handler_generation::{
        MODULES_BEGIN, MODULES_END, REGISTRATIONS_BEGIN, REGISTRATIONS_END, ensure_new_path,
        handler_template, rust_names, update_managed_section, validate_constructs_directory,
        write_new_file,
    },
    project::load_definition_project,
};

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
    ensure_new_path(&handler_path, "handler source")?;

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
    let typed_name = match project.manifest.rust_bindings {
        RustBindings::Dynamic => None,
        RustBindings::Typed => Some(langmeld_build::construct_type_name(&args.construct)?),
    };
    let handler_source = handler_template(&args.construct, &handler_name, typed_name.as_deref());

    write_new_file(&handler_path, &handler_source)?;
    if let Err(error) = replace_existing(&registry_path, &updated_registry) {
        let cleanup_error = fs::remove_file(&handler_path).err();
        let error = match cleanup_error {
            Some(cleanup_error) if cleanup_error.kind() != io::ErrorKind::NotFound => {
                io::Error::new(
                    error.kind(),
                    format!(
                        "{error}; rollback was incomplete: could not remove generated handler `{}`: {cleanup_error}",
                        handler_path.display()
                    ),
                )
            }
            _ => error,
        };
        return Err(error.into());
    }

    println!(
        "Created `src/constructs/{module_name}.rs` and registered `{}`.",
        args.construct
    );

    Ok(())
}
