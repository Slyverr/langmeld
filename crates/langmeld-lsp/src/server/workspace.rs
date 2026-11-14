use std::error::Error;

use lsp_server::{Connection, Message, Request, RequestId};
use lsp_types::{
    FileSystemWatcher, GlobPattern, Registration, RegistrationParams, Unregistration,
    UnregistrationParams,
};
use serde_json::json;

use super::{
    LanguageBackend, PendingWorkspaceRegistration, ServerState, WORKSPACE_REGISTRATION_REQUEST_ID,
    WatcherRegistration,
};

impl<B: LanguageBackend> ServerState<B> {
    pub(super) fn register_workspace_capabilities(
        &mut self,
        connection: &Connection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let register_configuration = self.workspace_capabilities.configuration_changes
            && self.client_supports_configuration_registration;
        let watchers = self.requested_watched_files();
        let watcher_registration = if watchers.is_empty() {
            None
        } else {
            Some(self.new_watcher_registration(watchers))
        };
        if !register_configuration && watcher_registration.is_none() {
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
        if let Some(registration) = &watcher_registration {
            registrations.push(watcher_registration_request(registration));
        }

        let request_id = RequestId::from(WORKSPACE_REGISTRATION_REQUEST_ID.to_owned());
        connection.sender.send(Message::Request(Request {
            id: request_id.clone(),
            method: "client/registerCapability".into(),
            params: serde_json::to_value(RegistrationParams { registrations })?,
        }))?;
        self.pending_workspace_registration = Some(PendingWorkspaceRegistration::Initialize {
            request_id,
            configuration_changes: register_configuration,
            watcher_registration,
            workspace_revision: self.workspace_revision,
        });

        Ok(())
    }

    pub(super) fn refresh_watched_file_registration(
        &mut self,
        connection: &Connection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.workspace_capabilities.watched_files =
            self.backend.workspace_capabilities().watched_files;
        self.workspace_revision = self.workspace_revision.saturating_add(1);

        self.reconcile_watched_files(connection)
    }

    pub(super) fn handle_workspace_response(
        &mut self,
        request_id: RequestId,
        succeeded: bool,
        connection: &Connection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let Some(pending) = self.pending_workspace_registration.take() else {
            return Ok(());
        };
        if pending.request_id() != &request_id {
            self.pending_workspace_registration = Some(pending);
            return Ok(());
        }

        let should_reconcile = match pending {
            PendingWorkspaceRegistration::Initialize {
                configuration_changes,
                watcher_registration,
                workspace_revision,
                ..
            } => {
                self.configuration_registered = succeeded && configuration_changes;
                self.registered_watcher_registration = if succeeded {
                    watcher_registration
                } else {
                    None
                };

                succeeded || self.workspace_revision > workspace_revision
            }
            PendingWorkspaceRegistration::UnregisterWatchers {
                registration,
                workspace_revision,
                ..
            } => {
                if succeeded
                    && self
                        .registered_watcher_registration
                        .as_ref()
                        .is_some_and(|current| current.id == registration.id)
                {
                    self.registered_watcher_registration = None;
                }

                succeeded || self.workspace_revision > workspace_revision
            }
            PendingWorkspaceRegistration::RegisterWatchers {
                registration,
                workspace_revision,
                ..
            } => {
                if succeeded {
                    self.registered_watcher_registration = Some(registration);
                }

                succeeded || self.workspace_revision > workspace_revision
            }
        };

        if should_reconcile {
            self.reconcile_watched_files(connection)?;
        }

        Ok(())
    }

    fn requested_watched_files(&self) -> Vec<FileSystemWatcher> {
        if !self.client_supports_file_watcher_registration {
            return Vec::new();
        }

        let has_unsupported_relative_patterns = !self
            .client_supports_relative_file_watcher_patterns
            && self
                .workspace_capabilities
                .watched_files
                .iter()
                .any(|watcher| matches!(watcher.glob_pattern, GlobPattern::Relative(_)));
        if has_unsupported_relative_patterns {
            Vec::new()
        } else {
            self.workspace_capabilities.watched_files.clone()
        }
    }

    fn reconcile_watched_files(
        &mut self,
        connection: &Connection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if self.pending_workspace_registration.is_some() {
            return Ok(());
        }

        let desired_watchers = self.requested_watched_files();
        if self
            .registered_watcher_registration
            .as_ref()
            .is_some_and(|current| current.watchers == desired_watchers)
        {
            return Ok(());
        }

        if let Some(registration) = self.registered_watcher_registration.clone() {
            self.unregister_watched_files(registration, connection)?;
        } else if !desired_watchers.is_empty() {
            self.register_watched_files(desired_watchers, connection)?;
        }

        Ok(())
    }

    fn unregister_watched_files(
        &mut self,
        registration: WatcherRegistration,
        connection: &Connection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let request_id = self.next_workspace_request_id();
        let params = UnregistrationParams {
            unregisterations: vec![Unregistration {
                id: registration.id.clone(),
                method: "workspace/didChangeWatchedFiles".into(),
            }],
        };
        connection.sender.send(Message::Request(Request {
            id: request_id.clone(),
            method: "client/unregisterCapability".into(),
            params: serde_json::to_value(params)?,
        }))?;
        self.pending_workspace_registration =
            Some(PendingWorkspaceRegistration::UnregisterWatchers {
                request_id,
                registration,
                workspace_revision: self.workspace_revision,
            });

        Ok(())
    }

    fn register_watched_files(
        &mut self,
        watchers: Vec<FileSystemWatcher>,
        connection: &Connection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let registration = self.new_watcher_registration(watchers);
        let request_id = self.next_workspace_request_id();
        let params = RegistrationParams {
            registrations: vec![watcher_registration_request(&registration)],
        };
        connection.sender.send(Message::Request(Request {
            id: request_id.clone(),
            method: "client/registerCapability".into(),
            params: serde_json::to_value(params)?,
        }))?;
        self.pending_workspace_registration =
            Some(PendingWorkspaceRegistration::RegisterWatchers {
                request_id,
                registration,
                workspace_revision: self.workspace_revision,
            });

        Ok(())
    }

    fn new_watcher_registration(
        &mut self,
        watchers: Vec<FileSystemWatcher>,
    ) -> WatcherRegistration {
        self.next_watcher_registration_id = self.next_watcher_registration_id.saturating_add(1);

        WatcherRegistration {
            id: format!(
                "langmeld-watched-files-{}",
                self.next_watcher_registration_id
            ),
            watchers,
        }
    }

    fn next_workspace_request_id(&mut self) -> RequestId {
        self.next_workspace_request_id = self.next_workspace_request_id.saturating_add(1);

        format!(
            "langmeld-workspace-watchers-{}",
            self.next_workspace_request_id
        )
        .into()
    }
}

fn watcher_registration_request(registration: &WatcherRegistration) -> Registration {
    Registration {
        id: registration.id.clone(),
        method: "workspace/didChangeWatchedFiles".into(),
        register_options: Some(json!({
            "watchers": &registration.watchers,
        })),
    }
}
