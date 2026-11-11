use std::error::Error;

use langmeld_core::{PositionEncoding, SourceFile, SourceId};
use lsp_server::{Connection, Message, Notification};
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    PublishDiagnosticsParams, TextDocumentContentChangeEvent, Uri,
};
use serde::de::DeserializeOwned;

use crate::conversion::{byte_offset, diagnostics_for};

use super::{LanguageBackend, OpenDocument, ServerState, WorkspaceRefreshTrigger};

impl<B: LanguageBackend> ServerState<B> {
    pub(super) fn open(
        &mut self,
        params: DidOpenTextDocumentParams,
        connection: &Connection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let item = params.text_document;
        let uri = item.uri;
        if self
            .documents
            .get(&uri)
            .is_some_and(|document| document.version > item.version)
        {
            return Ok(());
        }
        let source_id = self.sources.upsert(uri.as_str(), item.text)?;
        self.refresh(uri, source_id, item.version, connection)
    }

    pub(super) fn change(
        &mut self,
        params: DidChangeTextDocumentParams,
        connection: &Connection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let uri = params.text_document.uri;
        let Some(current) = self.documents.get(&uri) else {
            return Ok(());
        };
        if params.text_document.version <= current.version {
            return Ok(());
        }
        let Some(source) = self.sources.get(current.source_id) else {
            return Ok(());
        };
        let Some(text) =
            apply_content_changes(source.text.clone(), &params.content_changes, self.encoding)
        else {
            return Ok(());
        };
        let source_id = self.sources.upsert(uri.as_str(), text)?;
        self.refresh(uri, source_id, params.text_document.version, connection)
    }

    pub(super) fn close(
        &mut self,
        params: DidCloseTextDocumentParams,
        connection: &Connection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let uri = params.text_document.uri;
        if let Some(document) = self.documents.remove(&uri) {
            if let Some(source) = self.sources.get(document.source_id) {
                self.backend.did_close(source, &document.analysis);
            }
            self.sources.remove(document.source_id);
            publish_diagnostics(connection, uri, None, Vec::new())?;
            self.refresh_workspace(None, WorkspaceRefreshTrigger::DocumentClosed, connection)?;
        }
        Ok(())
    }

    fn refresh(
        &mut self,
        uri: Uri,
        source_id: SourceId,
        version: i32,
        connection: &Connection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.refresh_workspace(
            Some((uri, source_id, version)),
            WorkspaceRefreshTrigger::DocumentChanged,
            connection,
        )
    }

    pub(super) fn refresh_workspace(
        &mut self,
        updated: Option<(Uri, SourceId, i32)>,
        trigger: WorkspaceRefreshTrigger,
        connection: &Connection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let updated_uri = updated.as_ref().map(|(uri, _, _)| uri);
        let mut entries = self
            .documents
            .iter()
            .filter(|(uri, _)| Some(*uri) != updated_uri)
            .filter_map(|(uri, document)| {
                self.sources.get(document.source_id).map(|source| {
                    (
                        uri.clone(),
                        document.source_id,
                        document.version,
                        source.clone(),
                    )
                })
            })
            .collect::<Vec<_>>();
        if let Some((uri, source_id, version)) = &updated {
            let source = self
                .sources
                .get(*source_id)
                .expect("upserted document source exists")
                .clone();
            entries.push((uri.clone(), *source_id, *version, source));
        }
        entries.sort_by(|left, right| left.0.as_str().cmp(right.0.as_str()));

        let sources = entries
            .iter()
            .map(|(_, _, _, source)| source.clone())
            .collect::<Vec<_>>();
        let workspace = self.backend.analyze_workspace(&sources);
        let has_workspace_result = workspace.is_some();
        if let Some(analyses) = workspace.filter(|analyses| {
            analyses.len() == entries.len()
                && analyses
                    .iter()
                    .zip(&entries)
                    .all(|(analysis, (_, _, _, source))| analysis.source == source.identity())
        }) {
            for ((uri, source_id, version, source), analysis) in entries.into_iter().zip(analyses) {
                let diagnostics = diagnostics_for(&source, &analysis, self.encoding);
                self.documents.insert(
                    uri.clone(),
                    OpenDocument {
                        source_id,
                        version,
                        analysis,
                    },
                );
                publish_diagnostics(connection, uri, Some(version), diagnostics)?;
            }
            return Ok(());
        }

        if has_workspace_result {
            for (uri, source_id, version, source) in entries {
                let analysis = self.backend.analyze(&source);
                let diagnostics = diagnostics_for(&source, &analysis, self.encoding);
                self.documents.insert(
                    uri.clone(),
                    OpenDocument {
                        source_id,
                        version,
                        analysis,
                    },
                );
                publish_diagnostics(connection, uri, Some(version), diagnostics)?;
            }
            return Ok(());
        }

        let Some((uri, source_id, version)) = updated else {
            if !matches!(trigger, WorkspaceRefreshTrigger::WorkspaceChanged) {
                return Ok(());
            }
            for (uri, source_id, version, source) in entries {
                let analysis = self.backend.analyze(&source);
                let diagnostics = diagnostics_for(&source, &analysis, self.encoding);
                self.documents.insert(
                    uri.clone(),
                    OpenDocument {
                        source_id,
                        version,
                        analysis,
                    },
                );
                publish_diagnostics(connection, uri, Some(version), diagnostics)?;
            }
            return Ok(());
        };
        let source = self
            .sources
            .get(source_id)
            .expect("upserted document source exists")
            .clone();
        let analysis = self.backend.analyze(&source);
        let diagnostics = diagnostics_for(&source, &analysis, self.encoding);
        self.documents.insert(
            uri.clone(),
            OpenDocument {
                source_id,
                version,
                analysis,
            },
        );
        publish_diagnostics(connection, uri, Some(version), diagnostics)
    }
}

fn publish_diagnostics(
    connection: &Connection,
    uri: Uri,
    version: Option<i32>,
    diagnostics: Vec<lsp_types::Diagnostic>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let params = PublishDiagnosticsParams {
        uri,
        diagnostics,
        version,
    };
    connection.sender.send(Message::Notification(Notification {
        method: "textDocument/publishDiagnostics".into(),
        params: serde_json::to_value(params)?,
    }))?;
    Ok(())
}

pub(super) fn parse_notification<T: DeserializeOwned>(
    notification: &Notification,
    method: &str,
) -> Option<T> {
    (notification.method == method)
        .then(|| serde_json::from_value(notification.params.clone()).ok())
        .flatten()
}

fn apply_content_changes(
    mut text: String,
    changes: &[TextDocumentContentChangeEvent],
    encoding: PositionEncoding,
) -> Option<String> {
    for change in changes {
        let Some(range) = change.range.as_ref() else {
            text.clone_from(&change.text);
            continue;
        };
        let source = SourceFile::new("", text.clone());
        let start = byte_offset(&source, range.start, encoding)?;
        let end = byte_offset(&source, range.end, encoding)?;
        if start > end {
            return None;
        }
        text.replace_range(start..end, &change.text);
    }
    Some(text)
}
