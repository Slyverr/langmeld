use std::{error::Error, fs, io, path::PathBuf};

use clap::Args;
use langmeld_project::RustBindings;

use crate::{
    file_ops::{read_regular_file, replace_existing, write_new_file},
    handler_generation::{
        ensure_new_handler_path, find_handler_module_collision, handler_template,
        registration_method, rust_names, update_handler_registry, validate_constructs_directory,
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
    let colliding_construct = find_handler_module_collision(
        &args.construct,
        &module_name,
        project
            .language
            .constructs()
            .iter()
            .map(|construct| construct.name()),
    );
    ensure_new_handler_path(
        &handler_path,
        &args.construct,
        &module_name,
        colliding_construct.as_deref(),
    )?;

    let typed_name = match project.manifest.rust_bindings {
        RustBindings::Dynamic => None,
        RustBindings::Typed => Some(langmeld_build::construct_type_name(&args.construct)?),
    };

    let registry_path = constructs_directory.join("mod.rs");
    let registry_source = read_regular_file(&registry_path)?;
    let registration_line = format!(
        "handlers.{}({module_name}::{handler_name})?;",
        registration_method(typed_name.as_deref())
    );
    if registry_source
        .lines()
        .any(|line| line.trim() == registration_line)
    {
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
    let updated_registry =
        update_handler_registry(&registry_source, &module_line, &registration_line)
            .map_err(|message| io::Error::new(io::ErrorKind::InvalidData, message))?;
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
