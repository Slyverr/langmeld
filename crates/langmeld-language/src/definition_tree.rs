//! Generic-parser AST conversion for `.lm` language definitions.

mod constructs;
mod conversion;
mod fields;
mod helpers;

use langmeld_core::{Diagnostic, SourceFile, Span, ValidatedLanguageSpec};

use crate::{AstNode, DefinitionLimits, ParseLimits, PreparedLanguage};

pub(crate) fn parse_language_definition_with_meta(
    source: &SourceFile,
    meta_language: &ValidatedLanguageSpec,
) -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    parse_language_definition_with_meta_and_limits(
        source,
        meta_language,
        DefinitionLimits::default(),
    )
}

pub(crate) fn parse_language_definition_with_meta_and_limits(
    source: &SourceFile,
    meta_language: &ValidatedLanguageSpec,
    limits: DefinitionLimits,
) -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    if source.text.len() > limits.max_source_bytes {
        return Err(vec![Diagnostic::error(
            "LMDEF100",
            format!(
                "definition is {} bytes; parsing limit is {} bytes",
                source.text.len(),
                limits.max_source_bytes
            ),
            Span::new(0, source.text.len()),
        )]);
    }
    let prepared = PreparedLanguage::new(meta_language.clone())
        .map_err(|error| vec![Diagnostic::spec_error("LM1080", error.to_string())])?;
    parse_language_definition_with_prepared_and_limits(source, &prepared, limits)
}

pub(crate) fn parse_language_definition_modules_with_meta_and_limits(
    root: &SourceFile,
    modules: &[SourceFile],
    meta_language: &ValidatedLanguageSpec,
    limits: DefinitionLimits,
) -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    let prepared = PreparedLanguage::new(meta_language.clone())
        .map_err(|error| vec![Diagnostic::spec_error("LM1080", error.to_string())])?;
    parse_language_definition_modules_with_prepared(root, modules, &prepared, limits)
}

pub(crate) fn parse_language_definition_with_prepared_and_limits(
    source: &SourceFile,
    prepared: &PreparedLanguage,
    limits: DefinitionLimits,
) -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    let result = (|| {
        let root = parse_definition_tree(source, prepared, limits)?;
        reject_imports(&root, source)?;
        conversion::convert_language(&root, source)
    })();
    limit_diagnostics(result, limits.max_diagnostics, source.text.len())
}

pub(crate) fn parse_language_definition_modules_with_prepared(
    root_source: &SourceFile,
    modules: &[SourceFile],
    prepared: &PreparedLanguage,
    limits: DefinitionLimits,
) -> Result<ValidatedLanguageSpec, Vec<Diagnostic>> {
    let result = (|| {
        let mut parsed_modules = Vec::with_capacity(modules.len() + 1);
        let mut total_bytes = 0usize;
        for source in modules.iter().chain(std::iter::once(root_source)) {
            total_bytes = total_bytes.saturating_add(source.text.len());
            if total_bytes > limits.max_source_bytes {
                return Err(vec![Diagnostic::error(
                    "LMDEF100",
                    format!(
                        "definition project is {total_bytes} bytes; parsing limit is {} bytes",
                        limits.max_source_bytes
                    ),
                    Span::new(0, source.text.len()),
                )]);
            }
            let ast = parse_definition_tree(source, prepared, limits)?;
            reject_imports(&ast, source)?;
            parsed_modules.push((ast, source));
        }

        conversion::convert_language_modules(
            parsed_modules.iter().map(|(ast, source)| (ast, *source)),
        )
    })();
    limit_diagnostics(result, limits.max_diagnostics, root_source.text.len())
}

fn reject_imports(node: &AstNode, source: &SourceFile) -> Result<(), Vec<Diagnostic>> {
    if let Some(import) = node
        .children
        .iter()
        .find(|declaration| declaration.construct == "import_decl")
    {
        return Err(vec![Diagnostic::error(
            "LMDEF045",
            format!(
                "{}: definition imports require the file-based loader",
                source.name
            ),
            import.span,
        )]);
    }
    Ok(())
}

fn limit_diagnostics<T>(
    result: Result<T, Vec<Diagnostic>>,
    max_diagnostics: usize,
    source_len: usize,
) -> Result<T, Vec<Diagnostic>> {
    let Err(mut diagnostics) = result else {
        return result;
    };
    let limit = max_diagnostics.max(1);
    if diagnostics.len() > limit {
        diagnostics.truncate(limit.saturating_sub(1));
        diagnostics.push(Diagnostic::error(
            "LMDEF102",
            format!("definition diagnostics exceed the limit of {limit}"),
            Span::new(0, source_len),
        ));
    }
    Err(diagnostics)
}

fn parse_limits(limits: DefinitionLimits) -> ParseLimits {
    ParseLimits {
        max_source_bytes: limits.max_source_bytes,
        max_tokens: limits.max_tokens,
        ..ParseLimits::default()
    }
}

fn parse_definition_tree(
    source: &SourceFile,
    meta_language: &PreparedLanguage,
    limits: DefinitionLimits,
) -> Result<AstNode, Vec<Diagnostic>> {
    let parsed = meta_language
        .clone()
        .configure()
        .map_err(|error| vec![Diagnostic::spec_error("LM1080", error.to_string())])?
        .parse_with_limits(source, parse_limits(limits));
    if parsed
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == langmeld_core::Severity::Error)
    {
        let mut diagnostics = parsed
            .diagnostics
            .into_iter()
            .map(|mut diagnostic| {
                if diagnostic.code == "LM1300" {
                    diagnostic.code = "LMDEF100".to_owned();
                } else if diagnostic.code == "LM1301" {
                    diagnostic.code = "LMDEF101".to_owned();
                } else if diagnostic.code == "LM1205" {
                    diagnostic.code = "LMDEF044".to_owned();
                }
                diagnostic.message = format!("{}: {}", source.name, diagnostic.message);
                diagnostic
            })
            .collect::<Vec<_>>();
        let diagnostic_limit = limits.max_diagnostics.max(1);
        if diagnostics.len() > diagnostic_limit {
            diagnostics.truncate(diagnostic_limit.saturating_sub(1));
            diagnostics.push(Diagnostic::error(
                "LMDEF102",
                format!("definition diagnostics exceed the limit of {diagnostic_limit}"),
                Span::new(0, source.text.len()),
            ));
        }
        return Err(diagnostics);
    }
    if parsed.program.nodes.len() != 1 {
        return Err(vec![Diagnostic::error(
            "LMBOOT001",
            format!(
                "meta-language parser produced {} language declarations; expected one",
                parsed.program.nodes.len()
            ),
            Span::new(0, source.text.len()),
        )]);
    }
    parsed.program.nodes.into_iter().next().ok_or_else(|| {
        vec![Diagnostic::error(
            "LMBOOT001",
            "meta-language parser produced no language declaration",
            Span::new(0, source.text.len()),
        )]
    })
}
