//! Loading and composing imported language-definition sources.

use langmeld_core::{Diagnostic, SourceFile, Span, has_errors};

use crate::{ConstructSpec, LanguageSpec, LexicalRules, SpecError};

use super::unit::{ImportDirective, parse_definition_unit};
use super::{DefinitionResolver, DefinitionResult, DefinitionSource, MAX_DEFINITION_IMPORT_DEPTH};

/// Loads and composes a language root and its explicitly imported local modules.
///
/// The root file contains one `language` declaration. Imported files contain
/// top-level `construct` and `import` declarations. The resolver owns path
/// handling and must return a stable key for each logical source.
#[must_use]
pub fn parse_language_definition_project(
    root: DefinitionSource,
    resolver: &impl DefinitionResolver,
) -> DefinitionResult {
    let mut unit = parse_definition_unit(&root.source, true);
    let Some(language_name) = unit.language_name.take() else {
        return DefinitionResult {
            language: None,
            diagnostics: unit.diagnostics,
        };
    };
    let mut diagnostics = std::mem::take(&mut unit.diagnostics);
    if root.key.trim().is_empty() {
        diagnostics.push(Diagnostic::error(
            "LM-DEF-019",
            "root definition source key cannot be empty",
            &root.source,
            Span::new(0, 0),
        ));
    }
    let mut constructs = unit
        .constructs
        .into_iter()
        .map(|(construct, span)| (construct, root.source.clone(), span))
        .collect::<Vec<_>>();
    let root_constructs = unit.root_constructs;
    let mut loading = vec![root.key.clone()];
    let mut loaded = std::collections::HashSet::new();
    loaded.insert(root.key.clone());

    load_imports(
        &root,
        unit.imports,
        resolver,
        &mut loading,
        &mut loaded,
        &mut constructs,
        &mut diagnostics,
    );

    finish_definition(
        Some(language_name),
        constructs,
        root_constructs,
        unit.lexical_rules,
        unit.lexical_span,
        diagnostics,
        root.source,
    )
}

fn load_imports(
    importer: &DefinitionSource,
    imports: Vec<ImportDirective>,
    resolver: &impl DefinitionResolver,
    loading: &mut Vec<String>,
    loaded: &mut std::collections::HashSet<String>,
    constructs: &mut Vec<(ConstructSpec, SourceFile, Span)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for import in imports {
        if import.path.ends_with(".lm") {
            diagnostics.push(Diagnostic::warning(
                "LM-DEF-W001",
                "legacy `.lm` import; rename the file to `.langmeld` and update this import",
                &importer.source,
                import.span,
            ));
        }

        if loading.len() >= MAX_DEFINITION_IMPORT_DEPTH {
            diagnostics.push(Diagnostic::error(
                "LM-DEF-021",
                format!("maximum definition import depth ({MAX_DEFINITION_IMPORT_DEPTH}) exceeded"),
                &importer.source,
                import.span,
            ));
            continue;
        }
        let resolved = match resolver.resolve(&importer.key, &import.path) {
            Ok(resolved) => resolved,
            Err(error) => {
                diagnostics.push(Diagnostic::error(
                    "LM-DEF-018",
                    format!("cannot resolve import `{}`: {error}", import.path),
                    &importer.source,
                    import.span,
                ));
                continue;
            }
        };
        if resolved.key.trim().is_empty() {
            diagnostics.push(Diagnostic::error(
                "LM-DEF-019",
                format!("resolver returned an empty key for `{}`", import.path),
                &importer.source,
                import.span,
            ));
            continue;
        }
        if loading.contains(&resolved.key) {
            diagnostics.push(Diagnostic::error(
                "LM-DEF-020",
                format!("import cycle reaches `{}`", import.path),
                &importer.source,
                import.span,
            ));
            continue;
        }
        if !loaded.insert(resolved.key.clone()) {
            continue;
        }

        loading.push(resolved.key.clone());
        let mut unit = parse_definition_unit(&resolved.source, false);
        diagnostics.append(&mut unit.diagnostics);
        constructs.extend(
            unit.constructs
                .into_iter()
                .map(|(construct, span)| (construct, resolved.source.clone(), span)),
        );
        load_imports(
            &resolved,
            unit.imports,
            resolver,
            loading,
            loaded,
            constructs,
            diagnostics,
        );
        loading.pop();
    }
}

pub(super) fn finish_definition(
    language_name: Option<String>,
    constructs: Vec<(ConstructSpec, SourceFile, Span)>,
    root_constructs: Vec<(String, Span)>,
    lexical_rules: LexicalRules,
    lexical_span: Option<Span>,
    mut diagnostics: Vec<Diagnostic>,
    fallback_source: SourceFile,
) -> DefinitionResult {
    let Some(language_name) = language_name else {
        return DefinitionResult {
            language: None,
            diagnostics,
        };
    };
    let specs = constructs
        .iter()
        .map(|(construct, _, _)| construct.clone())
        .collect::<Vec<_>>();
    let language = match LanguageSpec::new(language_name, specs)
        .and_then(|language| language.with_lexical_rules(lexical_rules))
        .and_then(|language| {
            if root_constructs.is_empty() {
                Ok(language)
            } else {
                let names = root_constructs
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>();
                language.with_root_constructs(&names)
            }
        }) {
        Ok(language) => Some(language),
        Err(error) => {
            let target = match &error {
                SpecError::DuplicateConstruct(name) => constructs
                    .iter()
                    .filter(|(construct, _, _)| construct.name() == name)
                    .nth(1),
                _ => spec_error_construct(&error).and_then(|name| {
                    constructs
                        .iter()
                        .find(|(construct, _, _)| construct.name() == name)
                }),
            };
            let lexical_error = matches!(
                error,
                SpecError::InvalidLineComment(_)
                    | SpecError::InvalidBlockComment { .. }
                    | SpecError::DuplicateCommentStart(_)
            );
            let root_target = match &error {
                SpecError::UnknownRootConstruct(name) => root_constructs
                    .iter()
                    .find(|(root, _)| root == name)
                    .map(|(_, span)| (&fallback_source, *span)),
                SpecError::DuplicateRootConstruct(name) => root_constructs
                    .iter()
                    .filter(|(root, _)| root == name)
                    .nth(1)
                    .map(|(_, span)| (&fallback_source, *span)),
                _ => None,
            };
            let (source, span) = root_target
                .or_else(|| target.map(|(_, source, span)| (source, *span)))
                .unwrap_or_else(|| {
                    (
                        &fallback_source,
                        if lexical_error {
                            lexical_span.unwrap_or_else(|| Span::new(0, 0))
                        } else {
                            Span::new(0, 0)
                        },
                    )
                });
            diagnostics.push(Diagnostic::error(
                "LM-DEF-004",
                error.to_string(),
                source,
                span,
            ));
            None
        }
    };
    let language = language.filter(|_| !has_errors(&diagnostics));
    DefinitionResult {
        language,
        diagnostics,
    }
}

fn spec_error_construct(error: &SpecError) -> Option<&str> {
    match error {
        SpecError::EmptyConstructName
        | SpecError::NoConstructs
        | SpecError::EmptyLanguageName
        | SpecError::UnknownRootConstruct(_)
        | SpecError::DuplicateRootConstruct(_)
        | SpecError::InvalidLineComment(_)
        | SpecError::InvalidBlockComment { .. }
        | SpecError::DuplicateCommentStart(_) => None,
        SpecError::DuplicateConstruct(name)
        | SpecError::EmptyPattern(name)
        | SpecError::EmptyLiteral(name)
        | SpecError::EmptyVariantName(name)
        | SpecError::EmptyPatternGroupName(name)
        | SpecError::InvalidBlockDelimiter(name) => Some(name),
        SpecError::InvalidLiteral { construct, .. } | SpecError::EmptyFieldName(construct) => {
            Some(construct)
        }
        SpecError::InvalidExpressionTerminator { construct, .. }
        | SpecError::InvalidChoiceValue { construct, .. }
        | SpecError::DuplicateChoiceValue { construct, .. } => Some(construct),
        SpecError::DuplicateField { construct, .. }
        | SpecError::DuplicateVariantName { construct, .. }
        | SpecError::DuplicatePatternName { construct, .. }
        | SpecError::UnknownChildConstruct { construct, .. }
        | SpecError::DuplicateChildConstruct { construct, .. }
        | SpecError::EmptyCategoryName(construct)
        | SpecError::DuplicateImplementedCategory { construct, .. }
        | SpecError::DuplicateBlockCategory { construct, .. }
        | SpecError::UnknownBlockCategory { construct, .. }
        | SpecError::EmptyChoiceValues(construct)
        | SpecError::EmptyPatternGroup(construct)
        | SpecError::EmptyOptional(construct)
        | SpecError::EmptyRepetition(construct)
        | SpecError::AmbiguousPatternBoundary(construct)
        | SpecError::AmbiguousVariants(construct) => Some(construct),
        SpecError::AmbiguousConstructs(left, _) => Some(left),
    }
}
