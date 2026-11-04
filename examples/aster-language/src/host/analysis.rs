// Semantic callbacks derived from the language definition.
// Replace each TODO with language-specific validation or analysis.

// <langmeld:registry:semantic>
/// Registrations for `semantic_analysis` behavior declarations.
pub fn semantic_behavior_registrations()
-> Vec<langmeld::language::ConfiguredSemanticHookRegistration> {
    vec![]
}

/// Validates that host callbacks match the behavior declarations.
pub fn semantic_behavior_hooks(
    spec: &langmeld::core::ValidatedLanguageSpec,
) -> Result<
    Vec<langmeld::language::ConfiguredSemanticHook>,
    langmeld::language::BehaviorRegistrationError,
> {
    langmeld::language::ConfiguredSemanticHookRegistry::new(spec, semantic_behavior_registrations())
        .map(langmeld::language::ConfiguredSemanticHookRegistry::into_hooks)
}
// </langmeld:registry:semantic>

// <langmeld:registry:resolver>
/// Registrations for `symbol_resolution` behavior declarations.
pub fn resolver_behavior_registrations()
-> Vec<langmeld::language::WorkspaceSymbolResolverRegistration> {
    vec![]
}

/// Validates that host callbacks match the behavior declarations.
pub fn resolver_behavior_hooks(
    spec: &langmeld::core::ValidatedLanguageSpec,
) -> Result<
    Vec<langmeld::language::WorkspaceSymbolResolverHook>,
    langmeld::language::BehaviorRegistrationError,
> {
    langmeld::language::WorkspaceSymbolResolverRegistry::new(
        spec,
        resolver_behavior_registrations(),
    )
    .map(langmeld::language::WorkspaceSymbolResolverRegistry::into_hooks)
}
// </langmeld:registry:resolver>

// <langmeld:registry:workspace>
/// Registrations for `workspace_type_analysis` behavior declarations.
pub fn workspace_behavior_registrations()
-> Vec<langmeld::language::WorkspaceTypeAnalysisRegistration> {
    vec![langmeld::language::WorkspaceTypeAnalysisRegistration::new(
        "aster_numeric_types",
        Box::new(crate::type_check::numeric_types),
    )]
}

/// Validates that host callbacks match the behavior declarations.
pub fn workspace_behavior_hooks(
    spec: &langmeld::core::ValidatedLanguageSpec,
) -> Result<
    Vec<langmeld::language::WorkspaceTypeAnalysisHook>,
    langmeld::language::BehaviorRegistrationError,
> {
    langmeld::language::WorkspaceTypeAnalysisRegistry::new(spec, workspace_behavior_registrations())
        .map(langmeld::language::WorkspaceTypeAnalysisRegistry::into_hooks)
}
// </langmeld:registry:workspace>
