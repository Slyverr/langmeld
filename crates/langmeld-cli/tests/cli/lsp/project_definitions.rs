use super::*;
use serde_json::json;

#[path = "project_definitions/completions.rs"]
mod completions;
#[path = "project_definitions/imports.rs"]
mod imports;
#[path = "project_definitions/navigation.rs"]
mod navigation;
#[path = "project_definitions/workspace.rs"]
mod workspace;
