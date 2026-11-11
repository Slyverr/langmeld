use std::{env, fs, path::Path};

use langmeld_core::{SourceFile, format_diagnostics};
use langmeld_custom_lexer_example::{RawStringLexer, load_language};
use langmeld_language::{CapturedValue, parse_with_lexer};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let language = load_language(project_root)?;

    let source_path = env::args_os().nth(1);
    let (source_name, source_text) = match source_path {
        Some(path) => {
            let text = fs::read_to_string(&path)
                .map_err(|error| format!("failed to read `{}`: {error}", path.to_string_lossy()))?;
            (path.to_string_lossy().into_owned(), text)
        }
        None => (
            "<sample>".to_owned(),
            "say r#\"comments // inside the string stay text\"#; // this is a comment\n".to_owned(),
        ),
    };

    let source = SourceFile::new(source_name, source_text);
    let parsed = parse_with_lexer(&source, &language, &RawStringLexer);

    if !parsed.diagnostics().is_empty() {
        let formatted = format_diagnostics(std::slice::from_ref(&source), parsed.diagnostics());

        if !parsed.is_valid() {
            return Err(formatted);
        }

        eprintln!("{formatted}");
    }

    let Some(field) = parsed
        .constructs()
        .first()
        .and_then(|construct| construct.fields.first())
    else {
        return Err("the parser did not produce the expected `say` construct".to_owned());
    };

    match &field.value {
        CapturedValue::String(value) => println!("Captured raw string: {value}"),
        _ => return Err("the `text` field was not a string token".to_owned()),
    }
    Ok(())
}
