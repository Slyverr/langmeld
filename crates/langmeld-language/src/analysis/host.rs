use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

/// A custom semantic check. Hooks receive immutable parse results and can only emit diagnostics.
pub type SemanticHook = fn(&Program, &SourceFile, &mut Vec<Diagnostic>);

/// A configured per-file semantic check that can capture owned host state.
pub type ConfiguredSemanticHook =
    Box<dyn for<'source> Fn(&Program, &'source SourceFile, &mut Vec<Diagnostic>) + Send + Sync>;

/// A host per-file semantic callback registered under a language behavior ID.
pub struct SemanticHookRegistration {
    /// Identifier declared with `behavior id: semantic_analysis` in the language.
    pub id: String,
    /// Host implementation for the declared behavior.
    pub hook: SemanticHook,
}

impl SemanticHookRegistration {
    /// Creates a per-file semantic callback registration with an owned behavior ID.
    #[must_use]
    pub fn new(id: impl Into<String>, hook: SemanticHook) -> Self {
        Self {
            id: id.into(),
            hook,
        }
    }
}

/// A per-file semantic callback registration that can own project configuration.
pub struct ConfiguredSemanticHookRegistration {
    /// Identifier declared with `behavior id: semantic_analysis` in the language.
    pub id: String,
    /// Host implementation, including any captured configuration.
    pub hook: ConfiguredSemanticHook,
}

impl ConfiguredSemanticHookRegistration {
    /// Creates a configured callback registration from a function or closure.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        hook: impl for<'source> Fn(&Program, &'source SourceFile, &mut Vec<Diagnostic>)
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            hook: Box::new(hook),
        }
    }
}

/// A host type-analysis pass over all files and generic facts in one workspace.
///
/// Hooks run after generic declaration/reference resolution and field type-fact
/// collection. They can capture owned policy/configuration, return host-inferred
/// type facts for custom syntax, and emit source-indexed diagnostics for cross-file
/// checks.
pub type WorkspaceTypeAnalysisHook = Box<
    dyn for<'source> Fn(
            &ValidatedLanguageSpec,
            &[(&'source SourceFile, &'source Program)],
            &[TypeFact],
            &[ResolvedReference],
            &mut Vec<HostTypeFact>,
            &mut Vec<WorkspaceDiagnostic>,
        ) + Send
        + Sync,
>;

/// A source-spanned type inferred by a host for syntax outside Langmeld's field model.
///
/// Host facts are returned alongside generic [`TypeFact`] values and can drive editor
/// features such as inlay hints. `source_index` refers to the workspace file slice passed
/// to the analysis hook.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostTypeFact {
    /// Index into the analyzed input file slice.
    pub source_index: usize,
    /// Stable source identity when supplied by the caller.
    pub source_id: Option<SourceId>,
    /// Revision analyzed when supplied by the caller.
    pub source_revision: Option<u64>,
    /// Source range of the expression or value whose type was inferred.
    pub span: Span,
    /// Host-defined type label for diagnostics and editor presentation.
    pub type_name: String,
}

impl HostTypeFact {
    /// Creates a host fact before the analysis engine attaches any stable source identity.
    #[must_use]
    pub fn new(source_index: usize, span: Span, type_name: impl Into<String>) -> Self {
        Self {
            source_index,
            source_id: None,
            source_revision: None,
            span,
            type_name: type_name.into(),
        }
    }
}

/// A host workspace-analysis callback registered under its language behavior ID.
pub struct WorkspaceTypeAnalysisRegistration {
    /// Identifier declared with `behavior id: workspace_type_analysis` in the language.
    pub id: String,
    /// Host implementation for the declared behavior.
    pub hook: WorkspaceTypeAnalysisHook,
}

impl WorkspaceTypeAnalysisRegistration {
    /// Creates a workspace type-analysis callback registration with an owned behavior ID.
    #[must_use]
    pub fn new(id: impl Into<String>, hook: WorkspaceTypeAnalysisHook) -> Self {
        Self {
            id: id.into(),
            hook,
        }
    }
}

/// A host symbol-resolution callback registered under its language behavior ID.
pub struct WorkspaceSymbolResolverRegistration {
    /// Identifier declared with `behavior id: symbol_resolution` in the language.
    pub id: String,
    /// Host implementation for the declared behavior.
    pub hook: WorkspaceSymbolResolverHook,
}

impl WorkspaceSymbolResolverRegistration {
    /// Creates a workspace symbol-resolver registration with an owned behavior ID.
    #[must_use]
    pub fn new(id: impl Into<String>, hook: WorkspaceSymbolResolverHook) -> Self {
        Self {
            id: id.into(),
            hook,
        }
    }
}

/// A problem found while binding language-declared workspace behaviors to a host.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BehaviorRegistrationIssue {
    /// The same behavior ID was registered more than once.
    Duplicate(String),
    /// The language requires a behavior the host did not register.
    Missing(String),
    /// The host registered a behavior not declared by the language.
    Undeclared(String),
}

impl fmt::Display for BehaviorRegistrationIssue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate(id) => {
                write!(formatter, "behavior `{id}` is registered more than once")
            }
            Self::Missing(id) => write!(
                formatter,
                "language requires behavior `{id}` but the host did not register it"
            ),
            Self::Undeclared(id) => write!(
                formatter,
                "host registered behavior `{id}` but the language does not declare it"
            ),
        }
    }
}

/// All behavior-registration problems found during host initialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BehaviorRegistrationError {
    /// Sorted registration issues for deterministic startup reporting.
    pub issues: Vec<BehaviorRegistrationIssue>,
}

impl fmt::Display for BehaviorRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, issue) in self.issues.iter().enumerate() {
            if index > 0 {
                formatter.write_str("; ")?;
            }
            issue.fmt(formatter)?;
        }
        Ok(())
    }
}

impl Error for BehaviorRegistrationError {}

fn validate_behavior_registration_ids(
    spec: &ValidatedLanguageSpec,
    stage: langmeld_core::BehaviorStage,
    registered_ids: impl IntoIterator<Item = String>,
    duplicate_ids: impl IntoIterator<Item = String>,
) -> Result<(), BehaviorRegistrationError> {
    let registered_ids: BTreeSet<String> = registered_ids.into_iter().collect();
    let declared_ids: BTreeSet<String> = spec
        .spec()
        .behaviors
        .iter()
        .filter(|behavior| behavior.stage == stage)
        .map(|behavior| behavior.id.clone())
        .collect();
    let mut issues: Vec<_> = duplicate_ids
        .into_iter()
        .map(BehaviorRegistrationIssue::Duplicate)
        .collect();
    for id in declared_ids.difference(&registered_ids) {
        issues.push(BehaviorRegistrationIssue::Missing(id.clone()));
    }
    for id in registered_ids.difference(&declared_ids) {
        issues.push(BehaviorRegistrationIssue::Undeclared(id.clone()));
    }
    if issues.is_empty() {
        Ok(())
    } else {
        issues.sort();
        Err(BehaviorRegistrationError { issues })
    }
}

fn checked_behavior_hooks<T>(
    spec: &ValidatedLanguageSpec,
    stage: langmeld_core::BehaviorStage,
    registrations: impl IntoIterator<Item = (String, T)>,
) -> Result<Vec<T>, BehaviorRegistrationError> {
    let mut registered = BTreeMap::new();
    let mut duplicate_ids = Vec::new();
    for (id, hook) in registrations {
        if registered.insert(id.clone(), hook).is_some() {
            duplicate_ids.push(id);
        }
    }
    validate_behavior_registration_ids(spec, stage, registered.keys().cloned(), duplicate_ids)?;
    Ok(registered.into_values().collect())
}

/// A validated set of workspace behaviors required by a language.
pub struct WorkspaceTypeAnalysisRegistry {
    hooks: Vec<WorkspaceTypeAnalysisHook>,
}

impl WorkspaceTypeAnalysisRegistry {
    /// Checks required, duplicate, and undeclared behavior registrations.
    pub fn new(
        spec: &ValidatedLanguageSpec,
        registrations: impl IntoIterator<Item = WorkspaceTypeAnalysisRegistration>,
    ) -> Result<Self, BehaviorRegistrationError> {
        let hooks = checked_behavior_hooks(
            spec,
            langmeld_core::BehaviorStage::WorkspaceTypeAnalysis,
            registrations
                .into_iter()
                .map(|registration| (registration.id, registration.hook)),
        )?;
        Ok(Self { hooks })
    }

    /// Borrows the checked callbacks for a workspace-analysis call.
    #[must_use]
    pub fn hooks(&self) -> &[WorkspaceTypeAnalysisHook] {
        &self.hooks
    }

    /// Consumes the registry and returns its callbacks for host APIs that take ownership.
    #[must_use]
    pub fn into_hooks(self) -> Vec<WorkspaceTypeAnalysisHook> {
        self.hooks
    }
}

/// A validated set of workspace symbol resolvers required by a language.
pub struct WorkspaceSymbolResolverRegistry {
    hooks: Vec<WorkspaceSymbolResolverHook>,
}

impl WorkspaceSymbolResolverRegistry {
    /// Checks required, duplicate, and undeclared symbol-resolver registrations.
    pub fn new(
        spec: &ValidatedLanguageSpec,
        registrations: impl IntoIterator<Item = WorkspaceSymbolResolverRegistration>,
    ) -> Result<Self, BehaviorRegistrationError> {
        let hooks = checked_behavior_hooks(
            spec,
            langmeld_core::BehaviorStage::SymbolResolution,
            registrations
                .into_iter()
                .map(|registration| (registration.id, registration.hook)),
        )?;
        Ok(Self { hooks })
    }

    /// Borrows the checked resolvers for a workspace-analysis call.
    #[must_use]
    pub fn hooks(&self) -> &[WorkspaceSymbolResolverHook] {
        &self.hooks
    }

    /// Consumes the registry and returns its callbacks for host APIs that take ownership.
    #[must_use]
    pub fn into_hooks(self) -> Vec<WorkspaceSymbolResolverHook> {
        self.hooks
    }
}

/// A validated set of per-file semantic behaviors required by a language.
pub struct SemanticHookRegistry {
    hooks: Vec<SemanticHook>,
}

impl SemanticHookRegistry {
    /// Checks required, duplicate, and undeclared semantic hook registrations.
    pub fn new(
        spec: &ValidatedLanguageSpec,
        registrations: impl IntoIterator<Item = SemanticHookRegistration>,
    ) -> Result<Self, BehaviorRegistrationError> {
        let hooks = checked_behavior_hooks(
            spec,
            langmeld_core::BehaviorStage::SemanticAnalysis,
            registrations
                .into_iter()
                .map(|registration| (registration.id, registration.hook)),
        )?;
        Ok(Self { hooks })
    }

    /// Borrows the checked callbacks for an analysis call.
    #[must_use]
    pub fn hooks(&self) -> &[SemanticHook] {
        &self.hooks
    }

    /// Consumes the registry and returns its callbacks for APIs that take ownership.
    #[must_use]
    pub fn into_hooks(self) -> Vec<SemanticHook> {
        self.hooks
    }
}

/// A validated set of configured per-file semantic behaviors for a language.
pub struct ConfiguredSemanticHookRegistry {
    hooks: Vec<ConfiguredSemanticHook>,
}

impl ConfiguredSemanticHookRegistry {
    /// Checks required, duplicate, and undeclared configured semantic registrations.
    pub fn new(
        spec: &ValidatedLanguageSpec,
        registrations: impl IntoIterator<Item = ConfiguredSemanticHookRegistration>,
    ) -> Result<Self, BehaviorRegistrationError> {
        let hooks = checked_behavior_hooks(
            spec,
            langmeld_core::BehaviorStage::SemanticAnalysis,
            registrations
                .into_iter()
                .map(|registration| (registration.id, registration.hook)),
        )?;
        Ok(Self { hooks })
    }

    /// Borrows the checked callbacks for an analysis call.
    #[must_use]
    pub fn hooks(&self) -> &[ConfiguredSemanticHook] {
        &self.hooks
    }

    /// Consumes the registry and returns callbacks for host APIs that take ownership.
    #[must_use]
    pub fn into_hooks(self) -> Vec<ConfiguredSemanticHook> {
        self.hooks
    }
}
