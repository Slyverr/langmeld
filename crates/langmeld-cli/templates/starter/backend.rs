//! Rust-owned backend registration and dispatch for this language project.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use langmeld::{
    core::SourceFile,
    language::{
        ArtifactBackend, BackendDescriptor, BackendError, BackendInputs, BackendOptions,
        BackendSetting, CompilationArtifact, Program, WorkspaceArtifactBackend,
        WorkspaceBackendInputs, compile_with_backend, compile_workspace_with_backend,
        validate_backend_descriptor, validate_backend_settings,
    },
};

/// A backend implemented by this language project's Rust host.
pub enum RegisteredBackend {
    /// Lowers one source file.
    Single(Box<dyn ArtifactBackend>),
    /// Lowers an analyzed set of source files as one workspace.
    Workspace(Box<dyn WorkspaceArtifactBackend>),
}

struct BackendEntry {
    descriptor: BackendDescriptor,
    settings: Vec<BackendSetting>,
    backend: RegisteredBackend,
}

/// The validated backend set available to this language project's host.
pub struct BackendRegistry {
    entries: Vec<BackendEntry>,
}

impl BackendRegistry {
    /// Validates and stores one backend registration set.
    pub fn new(backends: Vec<RegisteredBackend>) -> Result<Self, BackendDispatchError> {
        let mut entries = Vec::with_capacity(backends.len());
        let mut ids = BTreeSet::new();
        for backend in backends {
            let descriptor = backend.descriptor();
            validate_backend_descriptor(&descriptor).map_err(BackendDispatchError::Backend)?;
            let settings = backend.settings();
            validate_backend_settings(&descriptor.id, &settings, &BTreeMap::new())
                .map_err(BackendDispatchError::Backend)?;
            if !ids.insert(descriptor.id.clone()) {
                return Err(BackendDispatchError::Registry(format!(
                    "backend ID `{}` is registered more than once",
                    descriptor.id
                )));
            }
            entries.push(BackendEntry {
                descriptor,
                settings,
                backend,
            });
        }
        Ok(Self { entries })
    }

    /// Builds a registry from this project's `registered_backends` function.
    pub fn from_project() -> Result<Self, BackendDispatchError> {
        Self::new(registered_backends())
    }

    /// Returns the validated descriptors in registration order.
    pub fn descriptors(&self) -> impl Iterator<Item = &BackendDescriptor> {
        self.entries.iter().map(|entry| &entry.descriptor)
    }

    /// Returns the setting schema declared by one registered backend.
    pub fn settings(&self, backend_id: &str) -> Result<&[BackendSetting], BackendDispatchError> {
        self.entry(backend_id)
            .map(|entry| entry.settings.as_slice())
    }

    /// Compiles a parsed workspace through one registered backend.
    pub fn compile(
        &self,
        backend_id: &str,
        files: &[(&SourceFile, &Program)],
        artifacts: BTreeMap<String, Vec<u8>>,
        options: &BackendOptions,
    ) -> Result<CompilationArtifact, BackendDispatchError> {
        let entry = self.entry(backend_id)?;
        compile_entry(entry, backend_id, files, artifacts, options)
    }

    fn entry(&self, backend_id: &str) -> Result<&BackendEntry, BackendDispatchError> {
        if let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.descriptor.id == backend_id)
        {
            return Ok(entry);
        }
        let mut available: Vec<_> = self
            .entries
            .iter()
            .map(|entry| entry.descriptor.id.as_str())
            .collect();
        available.sort_unstable();
        Err(BackendDispatchError::Registry(format!(
            "backend `{backend_id}` is not registered; available backends: {}",
            available.join(", ")
        )))
    }
}

/// A project backend selection failure or a validated backend compilation error.
#[derive(Debug)]
pub enum BackendDispatchError {
    /// The project's registry is empty, ambiguous, or does not contain the selected ID.
    Registry(String),
    /// Langmeld rejected backend metadata, options, inputs, or output.
    Backend(BackendError),
}

impl fmt::Display for BackendDispatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(error) => formatter.write_str(error),
            Self::Backend(error) => fmt::Display::fmt(error, formatter),
        }
    }
}

impl Error for BackendDispatchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Registry(_) => None,
            Self::Backend(error) => Some(error),
        }
    }
}

impl RegisteredBackend {
    /// Returns the backend's advertised ID, formats, and input requirements.
    #[must_use]
    pub fn descriptor(&self) -> BackendDescriptor {
        match self {
            Self::Single(backend) => backend.descriptor(),
            Self::Workspace(backend) => backend.descriptor(),
        }
    }

    /// Returns the settings supported by this backend implementation.
    #[must_use]
    pub fn settings(&self) -> Vec<BackendSetting> {
        match self {
            Self::Single(backend) => backend.settings(),
            Self::Workspace(backend) => backend.settings(),
        }
    }
}

/// Returns the backends available to this language project.
///
/// Add implementations here. Backend code is host-owned and is not described
/// by `.lm` declarations.
pub fn registered_backends() -> Vec<RegisteredBackend> {
    Vec::new()
}

/// Selects a registered backend and compiles a parsed source workspace.
///
/// This function validates backend IDs and delegates format, setting, IR,
/// diagnostics, artifact, and source-map checks to Langmeld's backend contract.
/// The caller remains responsible for parsing and analysis before dispatch.
pub fn compile_registered_backend(
    backend_id: &str,
    files: &[(&SourceFile, &Program)],
    artifacts: BTreeMap<String, Vec<u8>>,
    options: &BackendOptions,
) -> Result<CompilationArtifact, BackendDispatchError> {
    BackendRegistry::from_project()?.compile(backend_id, files, artifacts, options)
}

fn compile_entry(
    entry: &BackendEntry,
    backend_id: &str,
    files: &[(&SourceFile, &Program)],
    artifacts: BTreeMap<String, Vec<u8>>,
    options: &BackendOptions,
) -> Result<CompilationArtifact, BackendDispatchError> {
    match &entry.backend {
        RegisteredBackend::Single(backend) => {
            if files.len() != 1 {
                return Err(BackendDispatchError::Registry(format!(
                    "backend `{backend_id}` accepts one source file, received {}",
                    files.len()
                )));
            }
            let (source, program) = files[0];
            let mut inputs = BackendInputs::new(program).with_source(source);
            for (id, bytes) in artifacts {
                inputs = inputs.with_artifact(id, bytes);
            }
            compile_with_backend(backend.as_ref(), &inputs, options)
                .map_err(BackendDispatchError::Backend)
        }
        RegisteredBackend::Workspace(backend) => {
            let mut inputs = WorkspaceBackendInputs::new(files);
            for (id, bytes) in artifacts {
                inputs = inputs.with_artifact(id, bytes);
            }
            compile_workspace_with_backend(backend.as_ref(), &inputs, options)
                .map_err(BackendDispatchError::Backend)
        }
    }
}
