use std::{
    error::Error,
    fs, io,
    io::IsTerminal,
    path::{Path, PathBuf},
};

use clap::Args;
use dialoguer::{Input, Select, theme::ColorfulTheme};
use langmeld_core::SourceFile;
use langmeld_language::{language_definition_grammar, parse, quote_definition_string};

use crate::{
    atomic_file::replace_existing,
    generate_handler::{
        MODULES_BEGIN, MODULES_END, REGISTRATIONS_BEGIN, REGISTRATIONS_END, handler_template,
        rust_names, update_managed_section, validate_constructs_directory, write_new_file,
    },
    project::{DefinitionProjectContext, load_definition_project},
};

#[derive(Debug, Args)]
pub(crate) struct ConstructArgs {
    /// Stable construct name used by the definition and Rust handler.
    name: Option<String>,
    /// Syntax literal used by the generated declaration; prompts interactively
    /// and defaults to the name in noninteractive use.
    #[arg(long)]
    literal: Option<String>,
    /// Category accepted by a block in the language definition.
    #[arg(long)]
    category: Option<String>,
    /// Path to langmeld.toml.
    #[arg(long, default_value = "langmeld.toml")]
    manifest_path: PathBuf,
}

pub(crate) fn generate_construct(args: ConstructArgs) -> Result<(), Box<dyn Error>> {
    let name = construct_name(args.name)?;
    let (module_name, handler_name) = rust_names(&name)?;
    let literal = syntax_literal(args.literal, &name)?;
    let context = DefinitionProjectContext::new(&args.manifest_path)?;
    let project = load_definition_project(&args.manifest_path)?;
    if project
        .language
        .constructs()
        .iter()
        .any(|construct| construct.name() == name.as_str())
    {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("construct `{name}` is already declared"),
        )
        .into());
    }
    let category = choose_category(args.category, &project.language)?;
    let root_path = context.root.join(&context.manifest.definition);
    let source_directory = root_path
        .parent()
        .ok_or("language definition path has no parent directory")?;
    let syntax_directory = source_directory.join("constructs");
    let syntax_path = syntax_directory.join(format!("{name}.langmeld"));
    let handler_directory = context.root.join("src/constructs");
    validate_constructs_directory(&context.root.join("src"))?;
    validate_constructs_directory(&handler_directory)?;
    let handler_path = handler_directory.join(format!("{module_name}.rs"));
    ensure_new_path(&syntax_path)?;
    ensure_new_path(&handler_path)?;

    let registry_path = handler_directory.join("mod.rs");
    let registry_source = read_regular_file(&registry_path)?;
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

    let root_source = read_regular_file(&root_path)?;
    let import_path = format!("constructs/{name}.langmeld");
    let updated_root = add_root_import(&root_source, &import_path)?;
    let syntax_source = syntax_template(&name, &literal, &category);
    let handler_source = handler_template(&name, &handler_name);

    let mut created_directory = false;
    match fs::symlink_metadata(&syntax_directory) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("refusing to write into `{}`", syntax_directory.display()),
                )
                .into());
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(&syntax_directory)?;
            created_directory = true;
        }
        Err(error) => return Err(error.into()),
    }

    if let Err(error) = write_new_file(&syntax_path, &syntax_source) {
        let rollback_errors =
            rollback_generated_paths(&[], created_directory.then_some(syntax_directory.as_path()));
        return Err(with_rollback_errors(error, rollback_errors).into());
    }

    if let Err(error) = write_new_file(&handler_path, &handler_source) {
        let rollback_errors = rollback_generated_paths(
            &[syntax_path.as_path()],
            created_directory.then_some(syntax_directory.as_path()),
        );
        return Err(with_rollback_errors(error, rollback_errors).into());
    }

    if let Err(error) = replace_existing(&root_path, &updated_root) {
        let mut rollback_errors = rollback_generated_paths(
            &[handler_path.as_path(), syntax_path.as_path()],
            created_directory.then_some(syntax_directory.as_path()),
        );
        restore_file(
            &root_path,
            &root_source,
            "language definition",
            &mut rollback_errors,
        );

        return Err(with_rollback_errors(error, rollback_errors).into());
    }

    if let Err(error) = replace_existing(&registry_path, &updated_registry) {
        let mut rollback_errors = rollback_generated_paths(
            &[handler_path.as_path(), syntax_path.as_path()],
            created_directory.then_some(syntax_directory.as_path()),
        );
        restore_file(
            &registry_path,
            &registry_source,
            "handler registry",
            &mut rollback_errors,
        );
        restore_file(
            &root_path,
            &root_source,
            "language definition",
            &mut rollback_errors,
        );

        return Err(with_rollback_errors(error, rollback_errors).into());
    }

    let syntax_relative = syntax_path
        .strip_prefix(&context.root)
        .unwrap_or(&syntax_path);
    let handler_relative = handler_path
        .strip_prefix(&context.root)
        .unwrap_or(&handler_path);
    println!("Created syntax `{}`.", syntax_relative.display());
    println!("Created handler `{}`.", handler_relative.display());
    println!("Edit the syntax declaration and implement the handler behavior.");
    Ok(())
}

fn construct_name(requested: Option<String>) -> Result<String, io::Error> {
    let name = match requested {
        Some(name) => name,
        None if io::stdin().is_terminal() && io::stdout().is_terminal() => {
            Input::<String>::with_theme(&ColorfulTheme::default())
                .with_prompt("Stable construct name")
                .validate_with(|name: &String| validate_construct_name(name))
                .interact_text()?
        }
        None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "provide a construct name or run this command in an interactive terminal",
            ));
        }
    };

    validate_construct_name(&name)?;
    Ok(name)
}

fn syntax_literal(requested: Option<String>, name: &str) -> Result<String, io::Error> {
    let literal = match requested {
        Some(literal) => literal,
        None if io::stdin().is_terminal() && io::stdout().is_terminal() => {
            Input::<String>::with_theme(&ColorfulTheme::default())
                .with_prompt("Starter syntax literal (edit for compound syntax)")
                .default(name.to_owned())
                .interact_text()?
        }
        None => name.to_owned(),
    };

    if literal.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "syntax literal cannot be empty",
        ));
    }

    Ok(literal)
}

fn choose_category(
    requested: Option<String>,
    language: &langmeld_language::LanguageSpec,
) -> Result<String, io::Error> {
    let mut categories = language
        .constructs()
        .iter()
        .filter_map(|construct| construct.block_spec())
        .flat_map(|block| block.categories().iter().cloned())
        .collect::<Vec<_>>();
    categories.sort();
    categories.dedup();

    if let Some(category) = requested {
        if categories.contains(&category) {
            return Ok(category);
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "category `{category}` is not accepted by any language block; choose one of: {}",
                categories.join(", ")
            ),
        ));
    }

    match categories.as_slice() {
        [category] => Ok(category.clone()),
        [] => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "no block category is declared; add a block with `allow_category` or pass `--category` after declaring one",
        )),
        _ if io::stdin().is_terminal() && io::stdout().is_terminal() => {
            let selected = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Which block category should accept this construct?")
                .items(&categories)
                .default(0)
                .interact()?;
            Ok(categories[selected].clone())
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "the language accepts multiple block categories ({}); choose one with `--category`",
                categories.join(", ")
            ),
        )),
    }
}

fn add_root_import(source: &str, import_path: &str) -> Result<String, io::Error> {
    let parsed_source = SourceFile::new("language definition", source);
    let parsed = parse(&parsed_source, language_definition_grammar());
    let language = parsed
        .constructs()
        .iter()
        .find(|construct| construct.name == "language")
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "could not locate the language declaration in the root definition",
            )
        })?;
    let definition = &source[language.span.start..language.span.end];
    let closing_brace = definition.rfind('}').ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "could not locate the end of the language declaration",
        )
    })?;
    let insertion = language.span.start + closing_brace;
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut updated = source.to_owned();
    updated.insert_str(
        insertion,
        &format!("    import \"{import_path}\";{newline}"),
    );
    Ok(updated)
}

fn syntax_template(name: &str, literal: &str, category: &str) -> String {
    format!(
        "construct {} {{\n    syntax {{\n        literal {};\n    }}\n    implements {};\n}}\n",
        quote_definition_string(name),
        quote_definition_string(literal),
        quote_definition_string(category)
    )
}

fn validate_construct_name(name: &str) -> Result<(), io::Error> {
    rust_names(name).map(|_| ())
}

fn ensure_new_path(path: &Path) -> Result<(), io::Error> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("refusing to overwrite `{}`", path.display()),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn read_regular_file(path: &Path) -> Result<String, io::Error> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "refusing to update symlink or non-file `{}`",
                path.display()
            ),
        ));
    }
    fs::read_to_string(path)
}

fn rollback_generated_paths(files: &[&Path], directory: Option<&Path>) -> Vec<String> {
    let mut errors = Vec::new();

    for path in files {
        if let Err(error) = fs::remove_file(path)
            && error.kind() != io::ErrorKind::NotFound
        {
            errors.push(format!(
                "could not remove generated file `{}`: {error}",
                path.display()
            ));
        }
    }

    if let Some(directory) = directory
        && let Err(error) = fs::remove_dir(directory)
        && error.kind() != io::ErrorKind::NotFound
    {
        errors.push(format!(
            "could not remove generated directory `{}`: {error}",
            directory.display()
        ));
    }

    errors
}

fn restore_file(path: &Path, contents: &str, description: &str, errors: &mut Vec<String>) {
    if fs::read(path).is_ok_and(|current| current.as_slice() == contents.as_bytes()) {
        return;
    }

    if let Err(error) = replace_existing(path, contents) {
        errors.push(format!(
            "could not restore {description} `{}`: {error}",
            path.display()
        ));
    }
}

fn with_rollback_errors(error: io::Error, rollback_errors: Vec<String>) -> io::Error {
    if rollback_errors.is_empty() {
        return error;
    }

    io::Error::new(
        error.kind(),
        format!(
            "{error}; rollback was incomplete: {}",
            rollback_errors.join("; ")
        ),
    )
}
