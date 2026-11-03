use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, IsTerminal, Write},
    path::Path,
};

use clap::Args;
use inquire::{Confirm, Select, Text};
use langmeld::language::{
    BackendDescriptor, ProjectBuildConfig, load_project_manifest, validate_backend_settings,
    write_project_manifest,
};

#[derive(Debug, Args)]
#[command(group(clap::ArgGroup::new("format_action").multiple(false)))]
#[command(group(clap::ArgGroup::new("target_action").multiple(false)))]
#[command(group(clap::ArgGroup::new("source_map_action").multiple(false)))]
pub(super) struct ConfigureArgs {
    /// Select a Rust-host backend.
    #[arg(long)]
    backend: Option<String>,
    /// Set the selected backend's default output format.
    #[arg(long, group = "format_action")]
    format: Option<String>,
    /// Remove the saved output format.
    #[arg(long, group = "format_action")]
    clear_format: bool,
    /// Set a backend-specific target value.
    #[arg(long, group = "target_action")]
    target: Option<String>,
    /// Remove the saved target.
    #[arg(long, group = "target_action")]
    clear_target: bool,
    /// Enable source maps by default.
    #[arg(long, group = "source_map_action")]
    source_maps: bool,
    /// Disable source maps by default.
    #[arg(long, group = "source_map_action")]
    no_source_maps: bool,
    /// Remove the source-map preference and use the backend default.
    #[arg(long, group = "source_map_action")]
    clear_source_maps: bool,
    /// Set a declared backend setting as KEY=VALUE (repeatable).
    #[arg(long = "set", value_name = "KEY=VALUE")]
    settings: Vec<String>,
    /// Remove one saved backend setting (repeatable).
    #[arg(long = "clear-setting", value_name = "KEY")]
    clear_setting: Vec<String>,
    /// Remove all saved settings for the selected backend.
    #[arg(long)]
    clear_settings: bool,
    /// Ask for each backend default, even with redirected input.
    #[arg(long)]
    interactive: bool,
    /// Remove all backend defaults from langmeld.json.
    #[arg(long)]
    clear: bool,
}

pub(super) fn configure(args: ConfigureArgs) -> Result<(), String> {
    let manifest_path = Path::new("langmeld.json");
    let mut manifest = load_project_manifest(manifest_path)?;
    if args.clear {
        if args.interactive
            || args.backend.is_some()
            || args.format.is_some()
            || args.clear_format
            || args.target.is_some()
            || args.clear_target
            || args.source_maps
            || args.no_source_maps
            || args.clear_source_maps
            || !args.settings.is_empty()
            || !args.clear_setting.is_empty()
            || args.clear_settings
        {
            return Err("--clear cannot be combined with other configuration options".to_owned());
        }
        manifest.build = None;
        write_project_manifest(manifest_path, &manifest)?;
        println!("cleared backend defaults from langmeld.json");
        return Ok(());
    }

    let has_field_updates = args.format.is_some()
        || args.clear_format
        || args.target.is_some()
        || args.clear_target
        || args.source_maps
        || args.no_source_maps
        || args.clear_source_maps
        || !args.settings.is_empty()
        || !args.clear_setting.is_empty()
        || args.clear_settings;
    let interactive = args.interactive
        || (args.backend.is_none() && !has_field_updates && io::stdin().is_terminal());
    if interactive && has_field_updates {
        return Err("--interactive cannot be combined with explicit backend settings".to_owned());
    }
    if !interactive && !has_field_updates && args.backend.is_none() {
        return Err(
            "configure needs a terminal prompt, --interactive, or at least one backend setting"
                .to_owned(),
        );
    }

    let registry = brief_language::host::backend::BackendRegistry::from_project()
        .map_err(|error| error.to_string())?;
    let descriptors: Vec<_> = registry.descriptors().collect();
    if descriptors.is_empty() {
        return Err("no backends are registered in src/host/backend.rs".to_owned());
    }

    let current = manifest.build.clone();
    let backend_id = if interactive {
        let default = args
            .backend
            .as_deref()
            .or_else(|| current.as_ref().map(|config| config.backend.as_str()));
        prompt_backend(&descriptors, default)?
    } else {
        choose_backend(&descriptors, args.backend.as_deref(), current.as_ref())?
    };
    let descriptor = descriptors
        .iter()
        .copied()
        .find(|descriptor| descriptor.id == backend_id)
        .ok_or_else(|| format!("selected backend `{backend_id}` is no longer registered"))?;
    let setting_schema = registry
        .settings(&backend_id)
        .map_err(|error| error.to_string())?;
    let mut candidate = match current.as_ref() {
        Some(config) if config.backend == backend_id => config.clone(),
        _ => ProjectBuildConfig::new(backend_id.clone()),
    };

    if interactive {
        configure_interactively(descriptor, setting_schema, &mut candidate)?;
    } else {
        apply_explicit_updates(&args, setting_schema, &mut candidate)?;
    }

    validate_candidate(descriptor, setting_schema, &candidate)?;
    manifest.build = Some(candidate);
    write_project_manifest(manifest_path, &manifest)?;
    println!("updated backend defaults in langmeld.json");
    Ok(())
}

fn choose_backend(
    descriptors: &[&BackendDescriptor],
    requested: Option<&str>,
    current: Option<&ProjectBuildConfig>,
) -> Result<String, String> {
    if let Some(id) = requested.or_else(|| current.map(|config| config.backend.as_str())) {
        if descriptors.iter().any(|descriptor| descriptor.id == id) {
            return Ok(id.to_owned());
        }
        return Err(format!(
            "backend `{id}` is not registered; available backends: {}",
            backend_ids(descriptors)
        ));
    }
    if descriptors.len() == 1 {
        return Ok(descriptors[0].id.clone());
    }
    Err(format!(
        "select a backend with --backend; available backends: {}",
        backend_ids(descriptors)
    ))
}

fn prompt_backend(
    descriptors: &[&BackendDescriptor],
    default: Option<&str>,
) -> Result<String, String> {
    let choices: Vec<_> = descriptors
        .iter()
        .map(|descriptor| descriptor.id.clone())
        .collect();
    prompt_choice("Select a backend", &choices, default)
}

fn configure_interactively(
    descriptor: &BackendDescriptor,
    schema: &[langmeld::language::BackendSetting],
    candidate: &mut ProjectBuildConfig,
) -> Result<(), String> {
    let format_ids: Vec<_> = descriptor
        .formats
        .iter()
        .map(|format| format.id.clone())
        .collect();
    let format = prompt_choice(
        "Select a backend output format",
        &format_ids,
        candidate.format.as_deref(),
    )?;
    candidate.format = Some(format);

    let target_default = candidate.target.as_deref().unwrap_or("");
    let target = prompt_text(
        "Target (enter `none` to clear)",
        (!target_default.is_empty()).then_some(target_default),
    )?;
    if target.eq_ignore_ascii_case("none") {
        candidate.target = None;
    } else if !target.trim().is_empty() {
        candidate.target = Some(target.trim().to_owned());
    }

    if descriptor.supports_source_maps {
        candidate.source_maps = Some(prompt_bool(
            "Generate source maps by default?",
            candidate.source_maps.unwrap_or(false),
        )?);
    } else {
        candidate.source_maps = None;
    }

    let mut settings = candidate.settings.clone();
    for setting in schema {
        if let Some(description) = &setting.description {
            println!("{description}");
        }
        let current = settings
            .get(&setting.key)
            .or(setting.default.as_ref())
            .map(String::as_str);
        let prompt = format!("{} (enter `none` to clear)", setting.label);
        if setting.choices.is_empty() {
            let value = prompt_text(&prompt, current)?;
            if value.eq_ignore_ascii_case("none") {
                settings.remove(&setting.key);
            } else if !value.trim().is_empty() {
                settings.insert(setting.key.clone(), value);
            }
        } else {
            let mut choices: Vec<_> = setting
                .choices
                .iter()
                .map(|choice| format!("value: {choice}"))
                .collect();
            const BACKEND_DEFAULT: &str = "<use backend default>";
            choices.push(BACKEND_DEFAULT.to_owned());
            let default = current
                .map(|value| format!("value: {value}"))
                .unwrap_or_else(|| BACKEND_DEFAULT.to_owned());
            let selected = prompt_choice(&prompt, &choices, Some(&default))?;
            if selected == BACKEND_DEFAULT {
                settings.remove(&setting.key);
            } else if let Some(value) = selected.strip_prefix("value: ") {
                settings.insert(setting.key.clone(), value.to_owned());
            }
        }
    }
    candidate.settings = settings;
    Ok(())
}

fn apply_explicit_updates(
    args: &ConfigureArgs,
    schema: &[langmeld::language::BackendSetting],
    candidate: &mut ProjectBuildConfig,
) -> Result<(), String> {
    if let Some(format) = &args.format {
        candidate.format = Some(format.clone());
    } else if args.clear_format {
        candidate.format = None;
    }
    if let Some(target) = &args.target {
        candidate.target = Some(target.clone());
    } else if args.clear_target {
        candidate.target = None;
    }
    if args.source_maps {
        candidate.source_maps = Some(true);
    } else if args.no_source_maps {
        candidate.source_maps = Some(false);
    } else if args.clear_source_maps {
        candidate.source_maps = None;
    }

    if args.clear_settings && (!args.settings.is_empty() || !args.clear_setting.is_empty()) {
        return Err("--clear-settings cannot be combined with --set or --clear-setting".to_owned());
    }
    if args.clear_settings {
        candidate.settings.clear();
    } else {
        let mut values = BTreeMap::new();
        for setting in &args.settings {
            let (key, value) = parse_setting(setting)?;
            if values.insert(key.clone(), value).is_some() {
                return Err(format!("backend setting `{key}` may only be changed once"));
            }
        }
        let mut clear_keys = BTreeSet::new();
        for key in &args.clear_setting {
            validate_setting_key(key)?;
            if !clear_keys.insert(key.as_str()) {
                return Err(format!("backend setting `{key}` may only be changed once"));
            }
            if values.contains_key(key) {
                return Err(format!(
                    "backend setting `{key}` may not be both set and cleared"
                ));
            }
            if !schema.iter().any(|setting| setting.key == *key) {
                return Err(format!("backend does not declare a setting named `{key}`"));
            }
        }
        for key in clear_keys {
            candidate.settings.remove(key);
        }
        candidate.settings.extend(values);
    }
    Ok(())
}

fn validate_candidate(
    descriptor: &BackendDescriptor,
    schema: &[langmeld::language::BackendSetting],
    candidate: &ProjectBuildConfig,
) -> Result<(), String> {
    if candidate.backend != descriptor.id {
        return Err("candidate backend ID does not match the selected backend".to_owned());
    }
    if let Some(format) = &candidate.format
        && !descriptor
            .formats
            .iter()
            .any(|available| available.id == *format)
    {
        let formats = descriptor
            .formats
            .iter()
            .map(|format| format.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "backend `{}` does not advertise format `{format}`; available formats: {formats}",
            descriptor.id
        ));
    }
    if candidate
        .target
        .as_deref()
        .is_some_and(|target| target.trim().is_empty())
    {
        return Err("backend target must not be empty".to_owned());
    }
    if candidate.source_maps == Some(true) && !descriptor.supports_source_maps {
        return Err(format!(
            "backend `{}` does not support source maps",
            descriptor.id
        ));
    }
    validate_backend_settings(&descriptor.id, schema, &candidate.settings)
        .map_err(|error| error.to_string())
}

fn prompt_choice(label: &str, choices: &[String], default: Option<&str>) -> Result<String, String> {
    if choices.is_empty() {
        return Err(format!("{label} has no available choices"));
    }
    if io::stdin().is_terminal() {
        let prompt = match default {
            Some(default) => format!("{label} (current: {default})"),
            None => label.to_owned(),
        };
        let starting_cursor = default_choice_index(choices, default);
        return Select::new(&prompt, choices.to_vec())
            .with_starting_cursor(starting_cursor)
            .prompt()
            .map_err(|error| error.to_string());
    }

    println!("{label}:");
    for (index, choice) in choices.iter().enumerate() {
        println!("  {}. {choice}", index + 1);
    }
    let default_index = default_choice_index(choices, default) + 1;
    let answer = prompt_text("Choose", Some(&default_index.to_string()))?;
    if let Ok(index) = answer.parse::<usize>()
        && index > 0
        && let Some(choice) = choices.get(index - 1)
    {
        return Ok(choice.clone());
    }
    choices
        .iter()
        .find(|choice| choice.as_str() == answer)
        .cloned()
        .ok_or_else(|| format!("unknown selection `{answer}`"))
}

fn default_choice_index(choices: &[String], default: Option<&str>) -> usize {
    default
        .and_then(|value| choices.iter().position(|choice| choice == value))
        .unwrap_or(0)
}

fn prompt_bool(label: &str, default: bool) -> Result<bool, String> {
    if io::stdin().is_terminal() {
        return Confirm::new(label)
            .with_default(default)
            .prompt()
            .map_err(|error| error.to_string());
    }
    let default_label = if default { "Y/n" } else { "y/N" };
    let answer = prompt_text(label, Some(default_label))?;
    match answer.to_ascii_lowercase().as_str() {
        "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
        value if value == default_label.to_ascii_lowercase() => Ok(default),
        _ => Err("answer with `y` or `n`".to_owned()),
    }
}

fn prompt_text(label: &str, default: Option<&str>) -> Result<String, String> {
    if io::stdin().is_terminal() {
        let prompt = Text::new(label);
        let prompt = if let Some(default) = default {
            prompt.with_default(default)
        } else {
            prompt
        };
        return prompt.prompt().map_err(|error| error.to_string());
    }
    print!("{label}");
    if let Some(default) = default {
        print!(" [{default}]");
    }
    print!(": ");
    io::stdout()
        .flush()
        .map_err(|error| format!("cannot write prompt: {error}"))?;
    let mut value = String::new();
    io::stdin()
        .read_line(&mut value)
        .map_err(|error| format!("cannot read response: {error}"))?;
    let value = value.trim();
    Ok(if value.is_empty() {
        default.unwrap_or_default().to_owned()
    } else {
        value.to_owned()
    })
}

fn parse_setting(value: &str) -> Result<(String, String), String> {
    let Some((key, value)) = value.split_once('=') else {
        return Err("backend settings must use KEY=VALUE".to_owned());
    };
    validate_setting_key(key)?;
    Ok((key.to_owned(), value.to_owned()))
}

fn validate_setting_key(key: &str) -> Result<(), String> {
    if key.is_empty()
        || key.trim() != key
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(format!("invalid backend setting key `{key}`"));
    }
    Ok(())
}

fn backend_ids(descriptors: &[&BackendDescriptor]) -> String {
    descriptors
        .iter()
        .map(|descriptor| descriptor.id.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::default_choice_index;

    #[test]
    fn choices_start_on_the_saved_value_or_first_available_value() {
        let choices = ["text".to_owned(), "json".to_owned(), "wasm".to_owned()];

        assert_eq!(default_choice_index(&choices, Some("json")), 1);
        assert_eq!(default_choice_index(&choices, Some("unknown")), 0);
        assert_eq!(default_choice_index(&choices, None), 0);
    }
}
