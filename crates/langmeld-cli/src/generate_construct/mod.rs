use std::{error::Error, io, io::IsTerminal, path::PathBuf};

use clap::Args;
use dialoguer::{Input, Select, theme::ColorfulTheme};
use langmeld_core::SourceFile;
use langmeld_language::{
    ConstructSpec, PatternPart, language_definition_grammar, parse, quote_definition_string,
};
use langmeld_project::RustBindings;

use crate::{
    handler_generation::{
        MODULES_BEGIN, MODULES_END, REGISTRATIONS_BEGIN, REGISTRATIONS_END,
        find_handler_module_collision, handler_module_collision_error, handler_template,
        rust_names, update_managed_section, validate_constructs_directory,
    },
    project::load_definition_project,
};

mod transaction;

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
    let typed_name = match project.manifest.rust_bindings {
        RustBindings::Dynamic => None,
        RustBindings::Typed => {
            let typed_language = project
                .language
                .clone()
                .into_builder()
                .construct(
                    ConstructSpec::new(name.clone(), [PatternPart::literal(literal.clone())])
                        .implements(&[category.as_str()]),
                )
                .build()?;
            langmeld_build::generate_bindings(&typed_language)?;

            Some(langmeld_build::construct_type_name(&name)?)
        }
    };
    let root_path = project.root.join(&project.manifest.definition);
    let source_directory = root_path
        .parent()
        .ok_or("language definition path has no parent directory")?;
    let syntax_directory = source_directory.join("constructs");
    let syntax_path = syntax_directory.join(format!("{name}.langmeld"));
    let handler_directory = project.root.join("src/constructs");
    validate_constructs_directory(&project.root.join("src"))?;
    validate_constructs_directory(&handler_directory)?;
    let handler_path = handler_directory.join(format!("{module_name}.rs"));
    let colliding_construct = find_handler_module_collision(
        &name,
        &module_name,
        project
            .language
            .constructs()
            .iter()
            .map(|construct| construct.name()),
    );

    let registry_path = handler_directory.join("mod.rs");
    let registry_source = transaction::read_regular_file(&registry_path)?;
    let module_line = format!("mod {module_name};");
    if let Some(colliding_construct) = colliding_construct.as_deref()
        && registry_source
            .lines()
            .any(|line| line.trim() == module_line)
    {
        return Err(
            handler_module_collision_error(&name, colliding_construct, &module_name).into(),
        );
    }
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

    let root_source = transaction::read_regular_file(&root_path)?;
    let import_path = format!("constructs/{name}.langmeld");
    let updated_root = add_root_import(&root_source, &import_path)?;
    let syntax_source = syntax_template(&name, &literal, &category);
    let handler_source = handler_template(&name, &handler_name, typed_name.as_deref());

    transaction::apply(transaction::ConstructFiles {
        root_path: &root_path,
        root_source: &root_source,
        updated_root: &updated_root,
        registry_path: &registry_path,
        registry_source: &registry_source,
        updated_registry: &updated_registry,
        syntax_directory: &syntax_directory,
        syntax_path: &syntax_path,
        syntax_source: &syntax_source,
        handler_path: &handler_path,
        handler_source: &handler_source,
        construct_name: &name,
        handler_module_name: &module_name,
        colliding_construct: colliding_construct.as_deref(),
    })?;

    let syntax_relative = syntax_path
        .strip_prefix(&project.root)
        .unwrap_or(&syntax_path);
    let handler_relative = handler_path
        .strip_prefix(&project.root)
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
    let declaration_line_start = source[..language.span.start]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let declaration_prefix = &source[declaration_line_start..language.span.start];
    let declaration_indent = if is_indentation(declaration_prefix) {
        declaration_prefix
    } else {
        ""
    };
    let indent_unit = language
        .children
        .iter()
        .find_map(|child| {
            let child_indent = indentation_at(source, child.span.start)?;
            child_indent
                .strip_prefix(declaration_indent)
                .filter(|indent| !indent.is_empty())
        })
        .unwrap_or("    ");
    let import_indent = format!("{declaration_indent}{indent_unit}");
    let closing_line_start = source[..insertion].rfind('\n').map_or(0, |index| index + 1);
    let closing_line_prefix = &source[closing_line_start..insertion];
    let import = format!("{import_indent}import \"{import_path}\";{newline}");
    let mut updated = source.to_owned();
    if is_indentation(closing_line_prefix) {
        updated.insert_str(closing_line_start, &import);
    } else {
        let content_end = source[..insertion].trim_end_matches([' ', '\t']).len();
        updated.replace_range(content_end..insertion, "");
        updated.insert_str(
            content_end,
            &format!(
                "{newline}{import_indent}import \"{import_path}\";{newline}{declaration_indent}"
            ),
        );
    }

    Ok(updated)
}

fn is_indentation(value: &str) -> bool {
    value
        .chars()
        .all(|character| matches!(character, ' ' | '\t'))
}

fn indentation_at(source: &str, offset: usize) -> Option<&str> {
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let indentation = &source[line_start..offset];

    is_indentation(indentation).then_some(indentation)
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
