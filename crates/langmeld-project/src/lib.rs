//! Source loading for modular Langmeld language projects.
//!
//! This crate keeps source-storage policy outside the language parser.
//! [`DirectoryDefinitionResolver`] confines file imports to one project root;
//! [`InMemoryDefinitionResolver`] resolves embedded, virtual, or editor-owned
//! source snapshots without filesystem access; [`OverlayDefinitionResolver`]
//! combines a project directory with in-memory imports.
#![deny(missing_docs)]

mod definition_paths;
mod definition_project;
mod directory_resolver;
mod error;
mod manifest;
mod memory_resolver;
mod overlay_resolver;

pub use definition_project::{
    DefinitionProjectError, LanguageSpecLoadError, LoadedDefinitionProject,
    load_definition_project, load_definition_project_from_manifest, load_language_spec,
};
pub use directory_resolver::DirectoryDefinitionResolver;
pub use error::ProjectLoadError;
pub use manifest::{
    DEFAULT_DEFINITION_PATH, PROJECT_MANIFEST_FILE, ProjectManifest, ProjectManifestError,
    RustBindings, read_project_manifest, read_project_manifest_file,
};
pub use memory_resolver::InMemoryDefinitionResolver;
pub use overlay_resolver::OverlayDefinitionResolver;
