use langmeld_core::{Diagnostic, Severity, SourceFile, ValidatedLanguageSpec};
use langmeld_language::{
    ConfiguredLanguageParser, PreparedLanguage, analyze_workspace, load_language, load_project,
    load_source_files,
};
use serde_json::{Value, json};

use crate::cli::{OutputFormat, ValidationArgs};

pub(crate) fn check_workspace(arguments: ValidationArgs) -> Result<(), (u8, String)> {
    let (language, sources) = if let Some(project_path) = arguments.project {
        let project = load_project(&project_path).map_err(|error| (2, error))?;
        ensure_generic_check_support(&project.language)?;
        (project.language, project.sources)
    } else {
        let language_path = arguments
            .language
            .as_deref()
            .expect("Clap requires a language or project input");
        let language = load_language(language_path).map_err(|error| (2, error))?;
        ensure_generic_check_support(&language)?;
        let sources = load_source_files(&arguments.sources).map_err(|error| (2, error))?;
        (language, sources)
    };

    let prepared =
        PreparedLanguage::new(language.clone()).map_err(|error| (2, error.to_string()))?;
    let parser = prepared
        .with_custom_parsers(std::iter::empty())
        .map_err(|error| (2, error.to_string()))?;
    let diagnostics = validate_sources(&language, &parser, &sources);
    print_diagnostics(
        &sources,
        &diagnostics,
        language.warnings(),
        arguments.format,
    )
    .map_err(|error| (2, error))?;
    if diagnostics
        .iter()
        .flatten()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        Err((1, "source contains errors".to_owned()))
    } else {
        Ok(())
    }
}

fn ensure_generic_check_support(language: &ValidatedLanguageSpec) -> Result<(), (u8, String)> {
    if !language.spec().behaviors.is_empty() {
        return Err((
            2,
            "this language declares host analysis behaviors; use a Rust host integration that registers them"
                .to_owned(),
        ));
    }
    Ok(())
}

fn validate_sources(
    language: &ValidatedLanguageSpec,
    parser: &ConfiguredLanguageParser,
    sources: &[SourceFile],
) -> Vec<Vec<Diagnostic>> {
    let parsed: Vec<_> = sources.iter().map(|source| parser.parse(source)).collect();
    let files: Vec<_> = sources
        .iter()
        .zip(&parsed)
        .map(|(source, result)| (source, &result.program))
        .collect();
    let mut diagnostics: Vec<_> = parsed
        .iter()
        .map(|result| result.diagnostics.clone())
        .collect();
    for item in analyze_workspace(language, &files) {
        diagnostics[item.source_index].push(item.diagnostic);
    }
    diagnostics
}

fn print_diagnostics(
    sources: &[SourceFile],
    diagnostics: &[Vec<Diagnostic>],
    validation_warnings: &[langmeld_core::SpecWarning],
    format: OutputFormat,
) -> Result<(), String> {
    if sources.len() == 1 {
        return print_one_source(&sources[0], &diagnostics[0], validation_warnings, format);
    }
    match format {
        OutputFormat::Text => {
            print_validation_warnings(validation_warnings);
            for (source, findings) in sources.iter().zip(diagnostics) {
                print_one_source(source, findings, &[], OutputFormat::Text)?;
            }
        }
        OutputFormat::Json => {
            let files: Vec<Value> = sources
                .iter()
                .zip(diagnostics)
                .map(|(source, findings)| json!({"source": source.name, "diagnostics": findings}))
                .collect();
            let valid = diagnostics
                .iter()
                .flatten()
                .all(|diagnostic| diagnostic.severity != Severity::Error);
            print_json(&json!({
                "valid": valid,
                "validation_warnings": validation_warnings_json(validation_warnings),
                "files": files
            }))?;
        }
    }
    Ok(())
}

fn print_one_source(
    source: &SourceFile,
    diagnostics: &[Diagnostic],
    validation_warnings: &[langmeld_core::SpecWarning],
    format: OutputFormat,
) -> Result<(), String> {
    match format {
        OutputFormat::Text => {
            print_validation_warnings(validation_warnings);
            if diagnostics.is_empty() {
                println!("{}: no issues found", source.name);
            } else {
                for diagnostic in diagnostics {
                    println!("{}", format_diagnostic(source, diagnostic));
                }
            }
        }
        OutputFormat::Json => {
            let valid = diagnostics
                .iter()
                .all(|diagnostic| diagnostic.severity != Severity::Error);
            print_json(&json!({
                "valid": valid,
                "source": source.name,
                "diagnostics": diagnostics,
                "validation_warnings": validation_warnings_json(validation_warnings)
            }))?;
        }
    }
    Ok(())
}

fn print_validation_warnings(warnings: &[langmeld_core::SpecWarning]) {
    for warning in warnings {
        println!("{}", format_validation_warning(warning));
    }
}

fn format_validation_warning(warning: &langmeld_core::SpecWarning) -> String {
    format!(
        "language definition: warning[{}]: {}",
        warning.code, warning.message
    )
}

fn validation_warnings_json(warnings: &[langmeld_core::SpecWarning]) -> Value {
    json!(warnings)
}

fn print_json(value: &Value) -> Result<(), String> {
    let output = serde_json::to_string_pretty(value)
        .map_err(|error| format!("cannot serialize validation output: {error}"))?;
    println!("{output}");
    Ok(())
}

pub(crate) fn format_diagnostic(source: &SourceFile, diagnostic: &Diagnostic) -> String {
    let severity = match diagnostic.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    };
    if let Some(span) = diagnostic.span {
        let (line, column) = source.line_column(span.start);
        format!(
            "{}:{line}:{column}: {severity}[{}]: {}",
            source.name, diagnostic.code, diagnostic.message
        )
    } else {
        format!(
            "{}: {severity}[{}]: {}",
            source.name, diagnostic.code, diagnostic.message
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use langmeld_core::{ConstructBuilder, LanguageBuilder, SpecWarning};
    use langmeld_language::CustomConstructParser;

    #[test]
    fn generic_check_refuses_languages_that_need_host_callbacks() {
        let language = LanguageBuilder::new("HostOnly")
            .version("0.1.0")
            .root("program")
            .construct(
                ConstructBuilder::new("program")
                    .syntax("program {")
                    .custom_parser("parse-program"),
            )
            .build()
            .expect("test language is valid");
        let prepared = PreparedLanguage::new(language.clone()).expect("patterns are valid");
        let error = prepared
            .with_custom_parsers(std::iter::empty::<CustomConstructParser>())
            .expect_err("generic CLI cannot bind user host callbacks");
        assert!(error.to_string().contains("requires custom parser"));
        assert!(ensure_generic_check_support(&language).is_ok());
    }

    #[test]
    fn diagnostics_mark_errors_and_show_unicode_aware_source_locations() {
        let source = SourceFile::new("sample.calc", "π\ninvalid");
        let diagnostic =
            Diagnostic::error("LM1202", "unknown syntax", langmeld_core::Span::new(3, 10));
        assert_eq!(
            format_diagnostic(&source, &diagnostic),
            "sample.calc:2:1: error[LM1202]: unknown syntax"
        );
    }

    #[test]
    fn validation_warnings_remain_definition_level_in_text_and_json() {
        let warning = SpecWarning {
            code: "LM1096",
            message: "syntax conflict analysis for block `program` was incomplete".to_owned(),
        };

        assert_eq!(
            format_validation_warning(&warning),
            "language definition: warning[LM1096]: syntax conflict analysis for block `program` was incomplete"
        );
        assert_eq!(
            validation_warnings_json(std::slice::from_ref(&warning)),
            json!([{
                "code": "LM1096",
                "message": "syntax conflict analysis for block `program` was incomplete"
            }])
        );
    }
}
