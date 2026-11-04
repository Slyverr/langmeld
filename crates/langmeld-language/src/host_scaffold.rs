use langmeld_core::ValidatedLanguageSpec;

fn marked_host_block(key: &str, contents: &str) -> String {
    format!(
        "// <langmeld:{key}>\n{}\n// </langmeld:{key}>\n\n",
        contents.trim_end()
    )
}

fn callback_names<'a>(
    ids: impl IntoIterator<Item = &'a str>,
) -> std::collections::BTreeMap<String, String> {
    let mut names = std::collections::BTreeMap::new();
    let mut used = std::collections::BTreeSet::new();
    let mut sorted = ids.into_iter().collect::<Vec<_>>();
    sorted.sort_unstable();
    for id in sorted {
        let mut suffix = String::new();
        for character in id.chars() {
            if character.is_ascii_alphanumeric() || character == '_' {
                suffix.push(character);
            } else {
                use std::fmt::Write as _;
                let _ = write!(suffix, "_u{:x}_", character as u32);
            }
        }
        let base = format!("callback_{suffix}");
        let mut candidate = base.clone();
        let mut collision = 2;
        while !used.insert(candidate.clone()) {
            candidate = format!("{base}_{collision}");
            collision += 1;
        }
        names.insert(id.to_owned(), candidate);
    }
    names
}

/// Generates Rust callback and runtime scaffolds for a validated language.
///
/// The returned paths are relative to a generated-artifact root. The output is
/// deterministic for a given language model and contains stubs rather than
/// executable implementations.
pub fn generate_host_scaffold(
    language: &ValidatedLanguageSpec,
) -> std::collections::BTreeMap<String, String> {
    use langmeld_core::BehaviorStage;
    use std::fmt::Write as _;

    let spec = language.spec();
    let mut parser_source = String::from(
        "// Custom parser callbacks derived from the language definition.\n\
         // Replace each TODO with a parser that returns source-spanned nodes.\n\n",
    );
    let parsers = spec
        .constructs
        .iter()
        .filter_map(|construct| construct.custom_parser.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    let parser_names = callback_names(parsers.iter().copied());
    let mut registry = String::from(
        "/// Callbacks required by constructs using `custom_parser`.\npub fn custom_parser_callbacks() -> Vec<langmeld::language::CustomConstructParser> {\n    vec![\n",
    );
    for id in &parsers {
        let _ = writeln!(
            registry,
            "        langmeld::language::CustomConstructParser::new({id:?}, host_parser_{}),",
            parser_names[*id]
        );
    }
    registry.push_str("    ]\n}\n");
    parser_source.push_str(&marked_host_block("registry:parser", &registry));
    for id in &parsers {
        let mut callback = String::new();
        let body_key = format!("body:parser:{id}");
        let _ = writeln!(
            callback,
            "/// TODO: Parse the `{id}` construct and return source-spanned generic fields.\n/// Return `Ok(None)` only when this input is not the `{id}` construct. Once its\n/// syntax is recognized, return a diagnostic for malformed input.\n/// Attach a host-owned AST with `.with_host_data(...)` when Rust needs richer data.\nfn host_parser_{}(\n    _source: &langmeld::core::SourceFile,\n    _start: usize,\n) -> Result<Option<langmeld::language::CustomConstructMatch>, langmeld::core::Diagnostic> {{\n    // <langmeld:{body_key}>\n    Ok(None)\n    // </langmeld:{body_key}>\n}}\n",
            parser_names[*id],
        );
        parser_source.push_str(&marked_host_block(
            &format!("callback:parser:{id}"),
            &callback,
        ));
    }

    let expression_atom_parsers = spec
        .expressions
        .iter()
        .filter_map(|expression| expression.host_atom_parser.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    let mut expression_registry = String::from(
        "/// Callbacks required by expression grammars using host-defined atoms.\npub fn expression_atom_parser_callbacks() -> Vec<langmeld::language::ExpressionAtomParserRegistration> {\n    vec![\n",
    );
    for id in &expression_atom_parsers {
        let _ = writeln!(
            expression_registry,
            "        langmeld::language::ExpressionAtomParserRegistration::new({id:?}, host_expression_atom_parser_{id}),"
        );
    }
    expression_registry.push_str("    ]\n}\n");
    parser_source.push_str(&marked_host_block(
        "registry:expression_atom_parser",
        &expression_registry,
    ));
    for id in &expression_atom_parsers {
        let body_key = format!("body:expression_atom_parser:{id}");
        let callback = format!(
            "/// TODO: Recognize one custom atom for expression grammars that use `{id}`.\nfn host_expression_atom_parser_{id}(\n    _source: &langmeld::core::SourceFile,\n    _range: langmeld::core::Span,\n    _offset: usize,\n) -> Result<Option<langmeld::expression::CustomAtomToken>, langmeld::core::Diagnostic> {{\n    // <langmeld:{body_key}>\n    Ok(None)\n    // </langmeld:{body_key}>\n}}\n"
        );
        parser_source.push_str(&marked_host_block(
            &format!("callback:expression_atom_parser:{id}"),
            &callback,
        ));
    }

    let mut analysis_source = String::from(
        "// Semantic callbacks derived from the language definition.\n\
         // Replace each TODO with language-specific validation or analysis.\n\n",
    );
    for (stage, stem, registration, registry, hooks_type) in [
        (
            BehaviorStage::SemanticAnalysis,
            "semantic",
            "ConfiguredSemanticHookRegistration",
            "ConfiguredSemanticHookRegistry",
            "ConfiguredSemanticHook",
        ),
        (
            BehaviorStage::SymbolResolution,
            "resolver",
            "WorkspaceSymbolResolverRegistration",
            "WorkspaceSymbolResolverRegistry",
            "WorkspaceSymbolResolverHook",
        ),
        (
            BehaviorStage::WorkspaceTypeAnalysis,
            "workspace",
            "WorkspaceTypeAnalysisRegistration",
            "WorkspaceTypeAnalysisRegistry",
            "WorkspaceTypeAnalysisHook",
        ),
    ] {
        let ids: Vec<_> = spec
            .behaviors
            .iter()
            .filter(|behavior| behavior.stage == stage)
            .map(|behavior| behavior.id.as_str())
            .collect();
        let names = callback_names(ids.iter().copied());
        let mut registration_source = String::new();
        let _ = writeln!(
            registration_source,
            "/// Registrations for `{}` behavior declarations.\npub fn {stem}_behavior_registrations() -> Vec<langmeld::language::{registration}> {{\n    vec![",
            match stage {
                BehaviorStage::SemanticAnalysis => "semantic_analysis",
                BehaviorStage::SymbolResolution => "symbol_resolution",
                BehaviorStage::WorkspaceTypeAnalysis => "workspace_type_analysis",
            }
        );
        for id in &ids {
            let function = format!("host_{stem}_{}", names[*id]);
            if stage == BehaviorStage::SemanticAnalysis {
                let _ = writeln!(
                    registration_source,
                    "        langmeld::language::{registration}::new({id:?}, {function}),"
                );
            } else {
                let _ = writeln!(
                    registration_source,
                    "        langmeld::language::{registration}::new({id:?}, Box::new({function})),"
                );
            }
        }
        registration_source.push_str("    ]\n}\n\n");

        if stage == BehaviorStage::SemanticAnalysis {
            for id in &ids {
                let mut callback = String::new();
                let body_key = format!("body:semantic:{id}");
                let _ = writeln!(
                    callback,
                    "/// TODO: Implement the `{id}` per-file semantic behavior.\nfn host_semantic_{}(\n    _program: &langmeld::language::Program,\n    _source: &langmeld::core::SourceFile,\n    _diagnostics: &mut Vec<langmeld::core::Diagnostic>,\n) {{\n    // <langmeld:{body_key}>\n    // Add semantic checks here.\n    // </langmeld:{body_key}>\n}}\n",
                    names[*id],
                );
                analysis_source.push_str(&marked_host_block(
                    &format!("callback:semantic:{id}"),
                    &callback,
                ));
            }
        } else if stage == BehaviorStage::WorkspaceTypeAnalysis {
            for id in &ids {
                let mut callback = String::new();
                let body_key = format!("body:workspace:{id}");
                let _ = writeln!(
                    callback,
                    "/// TODO: Implement the `{id}` workspace-wide analysis behavior.\nfn host_workspace_{}(\n    _spec: &langmeld::core::ValidatedLanguageSpec,\n    _files: &[(&langmeld::core::SourceFile, &langmeld::language::Program)],\n    _facts: &[langmeld::language::TypeFact],\n    _references: &[langmeld::language::ResolvedReference],\n    _host_facts: &mut Vec<langmeld::language::HostTypeFact>,\n    _diagnostics: &mut Vec<langmeld::language::WorkspaceDiagnostic>,\n) {{\n    // <langmeld:{body_key}>\n    // Add workspace type analysis here.\n    // </langmeld:{body_key}>\n}}\n",
                    names[*id],
                );
                analysis_source.push_str(&marked_host_block(
                    &format!("callback:workspace:{id}"),
                    &callback,
                ));
            }
        } else {
            for id in &ids {
                let mut callback = String::new();
                let body_key = format!("body:resolver:{id}");
                let _ = writeln!(
                    callback,
                    "/// TODO: Implement the `{id}` workspace symbol-resolution behavior.\nfn host_resolver_{}(\n    _spec: &langmeld::core::ValidatedLanguageSpec,\n    _files: &[(&langmeld::core::SourceFile, &langmeld::language::Program)],\n    _analysis: &mut langmeld::language::WorkspaceAnalysis,\n) {{\n    // <langmeld:{body_key}>\n    // Add symbol resolution here.\n    // </langmeld:{body_key}>\n}}\n",
                    names[*id],
                );
                analysis_source.push_str(&marked_host_block(
                    &format!("callback:resolver:{id}"),
                    &callback,
                ));
            }
        }

        let _ = writeln!(
            registration_source,
            "/// Validates that host callbacks match the behavior declarations.\npub fn {stem}_behavior_hooks(\n    spec: &langmeld::core::ValidatedLanguageSpec,\n) -> Result<Vec<langmeld::language::{hooks_type}>, langmeld::language::BehaviorRegistrationError> {{\n    langmeld::language::{registry}::new(spec, {stem}_behavior_registrations()).map(langmeld::language::{registry}::into_hooks)\n}}\n"
        );
        analysis_source.push_str(&marked_host_block(
            &format!("registry:{stem}"),
            &registration_source,
        ));
    }

    let lexer_source = marked_host_block(
        "callback:lexer:custom_lexer",
        "/// Extend Langmeld's built-in lexer, or return `None` to use the language's configured lexer.\n/// Use `lex_source_with_lexical_spec` when adapting a declared comment policy.\n/// Custom tokens must have ordered, non-overlapping UTF-8 byte spans. Omit the EOF sentinel.\npub fn custom_lexer() -> Option<langmeld::language::CustomLexer> {\n    // <langmeld:body:lexer:custom_lexer>\n    None\n    // </langmeld:body:lexer:custom_lexer>\n}",
    );

    std::collections::BTreeMap::from([
        (
            "host/mod.rs".to_owned(),
            "pub mod analysis;\npub mod backend;\npub mod lexer;\npub mod parser;\npub mod runtime;\n".to_owned(),
        ),
        ("host/parser.rs".to_owned(), parser_source),
        ("host/analysis.rs".to_owned(), analysis_source),
        ("host/lexer.rs".to_owned(), lexer_source),
        (
            "host/runtime.rs".to_owned(),
            "use langmeld::core::ValidatedLanguageSpec;\nuse langmeld::language::{ConfiguredLanguageParser, PreparedLanguage};\n\n/// Binds the callbacks generated for this language to its validated model.\npub fn configured_parser(spec: ValidatedLanguageSpec) -> Result<ConfiguredLanguageParser, String> {\n    let prepared = PreparedLanguage::new(spec).map_err(|error| error.to_string())?;\n    let parser = prepared\n        .with_custom_parsers_and_expression_atom_parsers(\n            crate::host::parser::custom_parser_callbacks(),\n            crate::host::parser::expression_atom_parser_callbacks(),\n        )\n        .map_err(|error| error.to_string())?;\n    Ok(match crate::host::lexer::custom_lexer() {\n        Some(lexer) => parser.with_custom_lexer(lexer),\n        None => parser,\n    })\n}\n".to_owned(),
        ),
        (
            "host/README.md".to_owned(),
            r#"# Rust host modules

This generated module tree follows the language definition and Langmeld's extension APIs:

- `parser.rs` contains stubs for declared custom construct parsers and expression atom parsers. Return source-spanned generic fields or `CustomAtomToken` values that identify the host-owned syntax.
- `lexer.rs` is the host's optional custom-lexer implementation. Use `lex_source(source)` for the legacy `#` policy or `lex_source_with_lexical_spec(source, &policy)` when adapting declared comment rules.
- `analysis.rs` contains semantic, symbol-resolution, and workspace behavior stubs plus checked registries.
- `backend.rs` registers the project's Rust-defined single-file and workspace backends. This host-owned module is not generated from `.langmeld` declarations.
- `runtime.rs` binds parser and expression callbacks plus an optional custom lexer to a validated language.
- `mod.rs` connects the Rust modules.

At a custom parser's byte offset, return `Ok(None)` only when the input does not
start that construct; this lets another construct parser or the declarative
pattern try. Once the input has the construct's recognizable prefix, return a
diagnostic for malformed syntax instead of falling through. Successful matches
must include the construct's declared required fields and UTF-8 byte spans.

`langmeld init` seeds these modules into `src/host/`. For declarative
projects, `langmeld generate --install-host` refreshes these modules while
preserving marked callback implementations. Generated Rust host CLIs use
`cargo run -- generate` to refresh references under
`.langmeld/generated/host/`. Library-only projects can call
`generate_host_scaffold` and the safe artifact writer from Rust. The generic
CLI does not load Rust project code.

The starter exposes `pub mod host;`, binds parser and lexer callbacks through
`host::runtime::configured_parser`, and calls the checked analysis registries
from its project-aware CLI. Add backend implementations to the user-owned
`src/host/backend.rs`; `.langmeld` declarations do not select executable backends.

The generated host tree contains callback references derived from the language
definition. Keep implementations in `src/host/`; files under
`.langmeld/generated/host/` are refreshable references.
"#
                .to_owned(),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::callback_names;

    #[test]
    fn callback_names_are_valid_deterministic_and_collision_safe() {
        let first = callback_names(["a-b", "a_u2d_b", "type", "λ-parser"]);
        let second = callback_names(["λ-parser", "type", "a_u2d_b", "a-b"]);

        assert_eq!(first, second);
        assert_eq!(first["a-b"], "callback_a_u2d_b");
        assert_eq!(first["a_u2d_b"], "callback_a_u2d_b_2");
        assert_eq!(first["type"], "callback_type");
        assert_eq!(first["λ-parser"], "callback__u3bb__u2d_parser");
        assert_eq!(
            first
                .values()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            4
        );
    }
}
