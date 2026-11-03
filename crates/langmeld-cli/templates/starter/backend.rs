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
        CompilationArtifact, Program, WorkspaceArtifactBackend, WorkspaceBackendInputs,
        compile_with_backend, compile_workspace_with_backend, validate_backend_descriptor,
    },
};

/// A backend implemented by this language project's Rust host.
pub enum RegisteredBackend {
    /// Lowers one source file.
    Single(Box<dyn ArtifactBackend>),
    /// Lowers an analyzed set of source files as one workspace.
    Workspace(Box<dyn WorkspaceArtifactBackend>),
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
    let backends = registered_backends();
    if backends.is_empty() {
        return Err(BackendDispatchError::Registry(
            "no backends are registered; add one to src/host/backend.rs before building".to_owned(),
        ));
    }

    let mut ids = BTreeSet::new();
    for backend in &backends {
        let descriptor = backend.descriptor();
        validate_backend_descriptor(&descriptor).map_err(BackendDispatchError::Backend)?;
        if !ids.insert(descriptor.id.clone()) {
            return Err(BackendDispatchError::Registry(format!(
                "backend ID `{}` is registered more than once",
                descriptor.id
            )));
        }
    }
    let backend = backends
        .iter()
        .find(|backend| backend.descriptor().id == backend_id)
        .ok_or_else(|| {
            BackendDispatchError::Registry(format!(
                "backend `{backend_id}` is not registered; available backends: {}",
                ids.into_iter().collect::<Vec<_>>().join(", ")
            ))
        })?;

    let result = match backend {
        RegisteredBackend::Single(backend) => {
            if files.len() != 1 {
                return Err(BackendDispatchError::Registry(format!(
                    "backend `{backend_id}` accepts one source file; received {}",
                    files.len()
                )));
            }
            let (source, program) = files[0];
            let mut inputs = BackendInputs::new(program).with_source(source);
            for (id, bytes) in artifacts {
                inputs = inputs.with_artifact(id, bytes);
            }
            compile_with_backend(backend.as_ref(), &inputs, options)
        }
        RegisteredBackend::Workspace(backend) => {
            let mut inputs = WorkspaceBackendInputs::new(files);
            for (id, bytes) in artifacts {
                inputs = inputs.with_artifact(id, bytes);
            }
            compile_workspace_with_backend(backend.as_ref(), &inputs, options)
        }
    };
    result.map_err(BackendDispatchError::Backend)
}
