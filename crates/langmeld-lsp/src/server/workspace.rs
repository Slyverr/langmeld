use std::error::Error;

use lsp_server::{Connection, Message, Request};
use lsp_types::{Registration, RegistrationParams};
use serde_json::json;

use super::{
    LanguageBackend, PendingWorkspaceRegistration, ServerState, WORKSPACE_REGISTRATION_REQUEST_ID,
};

impl<B: LanguageBackend> ServerState<B> {
    pub(super) fn register_workspace_capabilities(
        &mut self,
        connection: &Connection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let register_configuration = self.workspace_capabilities.configuration_changes
            && self.client_supports_configuration_registration;
        let has_unsupported_relative_patterns = !self
            .client_supports_relative_file_watcher_patterns
            && self
                .workspace_capabilities
                .watched_files
                .iter()
                .any(|watcher| matches!(watcher.glob_pattern, lsp_types::GlobPattern::Relative(_)));
        let register_file_watchers = !self.workspace_capabilities.watched_files.is_empty()
            && self.client_supports_file_watcher_registration
            && !has_unsupported_relative_patterns;
        if !register_configuration && !register_file_watchers {
            return Ok(());
        }

        let mut registrations = Vec::new();
        if register_configuration {
            registrations.push(Registration {
                id: "langmeld-configuration".into(),
                method: "workspace/didChangeConfiguration".into(),
                register_options: None,
            });
        }
        if register_file_watchers {
            registrations.push(Registration {
                id: "langmeld-watched-files".into(),
                method: "workspace/didChangeWatchedFiles".into(),
                register_options: Some(json!({
                    "watchers": &self.workspace_capabilities.watched_files,
                })),
            });
        }
        connection.sender.send(Message::Request(Request {
            id: WORKSPACE_REGISTRATION_REQUEST_ID.to_owned().into(),
            method: "client/registerCapability".into(),
            params: serde_json::to_value(RegistrationParams { registrations })?,
        }))?;
        self.pending_workspace_registration = Some(PendingWorkspaceRegistration {
            configuration_changes: register_configuration,
            watched_files: register_file_watchers,
        });
        Ok(())
    }
}
