use std::{
    collections::{HashMap, HashSet},
    error::Error,
};

use langmeld_core::{Diagnostic, PositionEncoding, SourceFile, SourceId, SourceIdentity, Span};
use lsp_server::{Connection, Message, Notification};
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    PublishDiagnosticsParams, TextDocumentContentChangeEvent, Uri,
};
use serde::de::DeserializeOwned;

use super::{LanguageBackend, OpenDocument, ServerState, WorkspaceRefreshTrigger};
use crate::backend::BackendDocument;
use crate::conversion::{byte_offset, diagnostic_for, diagnostics_for, diagnostics_for_source};

const WORKSPACE_RESULT_CODE: &str = "LM-LSP-001";
const DOCUMENT_SOURCE_MISMATCH_CODE: &str = "LM-LSP-002";
const DIAGNOSTIC_LOCATION_CODE: &str = "LM-LSP-003";
const WORKSPACE_SOURCE_MISMATCH_CODE: &str = "LM-LSP-004";

struct WorkspaceEntry {
    uri: Uri,
    source_id: SourceId,
    version: i32,
    source: SourceFile,
}

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
        let Some(text) = apply_content_changes(
            source.text().to_owned(),
            &params.content_changes,
            self.encoding,
        ) else {
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

    fn publish_analysis(
        &mut self,
        entry: WorkspaceEntry,
        analysis: BackendDocument<B::Document>,
        fallback_diagnostic: Option<Diagnostic>,
        connection: &Connection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let WorkspaceEntry {
            uri,
            source_id,
            version,
            source,
        } = entry;
        let converted = diagnostics_for(&source, &analysis, self.encoding);
        let mut diagnostics = converted.diagnostics;

        if converted.has_unpublishable_diagnostics {
            let unpublishable_diagnostics = Diagnostic::error(
                DIAGNOSTIC_LOCATION_CODE,
                "host language backend returned document diagnostics with missing, foreign, or invalid source locations; those diagnostics were suppressed",
                &source,
                Span::new(0, source.text().len()),
            );
            if let Some(diagnostic) =
                diagnostic_for(&source, &unpublishable_diagnostics, self.encoding)
            {
                diagnostics.push(diagnostic);
            }
        }

        if !analysis.matches_source(&source) {
            let mismatch = Diagnostic::error(
                DOCUMENT_SOURCE_MISMATCH_CODE,
                "host language backend returned a document for a different source snapshot; its diagnostics and editor queries were suppressed",
                &source,
                Span::new(0, source.text().len()),
            );
            if let Some(diagnostic) = diagnostic_for(&source, &mismatch, self.encoding) {
                diagnostics.push(diagnostic);
            }
        }

        if let Some(diagnostic) = fallback_diagnostic
            .as_ref()
            .and_then(|diagnostic| diagnostic_for(&source, diagnostic, self.encoding))
        {
            diagnostics.push(diagnostic);
        }

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
                self.sources
                    .get(document.source_id)
                    .map(|source| WorkspaceEntry {
                        uri: uri.clone(),
                        source_id: document.source_id,
                        version: document.version,
                        source: source.clone(),
                    })
            })
            .collect::<Vec<_>>();
        if let Some((uri, source_id, version)) = &updated {
            let source = self
                .sources
                .get(*source_id)
                .expect("upserted document source exists")
                .clone();
            entries.push(WorkspaceEntry {
                uri: uri.clone(),
                source_id: *source_id,
                version: *version,
                source,
            });
        }
        entries.sort_by(|left, right| left.uri.as_str().cmp(right.uri.as_str()));

        let sources = entries
            .iter()
            .map(|entry| entry.source.clone())
            .collect::<Vec<_>>();
        if let Some(analyses) = self.backend.analyze_workspace(&sources) {
            let invalid_batch = if analyses.len() != entries.len() {
                Some(format!(
                    "host language backend returned {} workspace results for {} open documents; per-document analysis was used instead",
                    analyses.len(),
                    entries.len(),
                ))
            } else {
                analyses
                    .iter()
                    .zip(&entries)
                    .position(|(analysis, entry)| !analysis.matches_source(&entry.source))
                    .map(|index| {
                        format!(
                            "host language backend returned a result for a different source snapshot at document {}; per-document analysis was used instead",
                            index + 1,
                        )
                    })
            };

            if let Some(message) = invalid_batch {
                for entry in entries {
                    let analysis = self.backend.analyze(&entry.source);
                    let fallback_diagnostic = Some(Diagnostic::error(
                        WORKSPACE_RESULT_CODE,
                        message.clone(),
                        &entry.source,
                        Span::new(0, entry.source.text().len()),
                    ));

                    self.publish_analysis(entry, analysis, fallback_diagnostic, connection)?;
                }
            } else {
                for (entry, analysis) in entries.into_iter().zip(analyses) {
                    self.publish_analysis(entry, analysis, None, connection)?;
                }
            }
        } else if let Some((uri, source_id, version)) = updated {
            let source = self
                .sources
                .get(source_id)
                .expect("upserted document source exists")
                .clone();
            let analysis = self.backend.analyze(&source);
            self.publish_analysis(
                WorkspaceEntry {
                    uri,
                    source_id,
                    version,
                    source,
                },
                analysis,
                None,
                connection,
            )?;
        } else if matches!(trigger, WorkspaceRefreshTrigger::WorkspaceChanged) {
            for entry in entries {
                let analysis = self.backend.analyze(&entry.source);
                self.publish_analysis(entry, analysis, None, connection)?;
            }
        }

        self.publish_workspace_diagnostics(&sources, connection)
    }

    pub(super) fn publish_workspace_diagnostics(
        &mut self,
        open_sources: &[SourceFile],
        connection: &Connection,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let open_uris = self
            .documents
            .keys()
            .map(|uri| uri.as_str().to_owned())
            .collect::<HashSet<_>>();

        self.workspace_diagnostic_uris
            .retain(|uri_key, _| !open_uris.contains(uri_key));

        let mut source_identities = HashMap::<String, SourceIdentity>::new();
        let mut diagnostics_by_uri = HashMap::<String, (Uri, Vec<lsp_types::Diagnostic>)>::new();
        let mut conflicting_uris = HashSet::new();

        let Some(reports) = self.backend.workspace_diagnostics(open_sources) else {
            return Ok(());
        };

        for report in reports {
            let external_source = report.source();
            let uri = external_source.uri().clone();
            let uri_key = uri.as_str().to_owned();
            let source = external_source.source();

            if open_uris.contains(&uri_key) || conflicting_uris.contains(&uri_key) {
                continue;
            }

            if source_identities
                .get(&uri_key)
                .is_some_and(|identity| *identity != source.identity())
            {
                let conflict = Diagnostic::error(
                    WORKSPACE_SOURCE_MISMATCH_CODE,
                    "host language backend returned multiple source snapshots for one workspace diagnostic URI; those findings were suppressed",
                    source,
                    Span::new(0, source.text().len()),
                );
                let diagnostic = diagnostic_for(source, &conflict, self.encoding)
                    .expect("the full source span is a valid diagnostic location");
                diagnostics_by_uri.insert(uri_key.clone(), (uri, vec![diagnostic]));
                conflicting_uris.insert(uri_key);
                continue;
            }

            source_identities
                .entry(uri_key.clone())
                .or_insert_with(|| source.identity());

            let converted = diagnostics_for_source(source, report.diagnostics(), self.encoding);
            let mut diagnostics = converted.diagnostics;
            if converted.has_unpublishable_diagnostics {
                let invalid_locations = Diagnostic::error(
                    DIAGNOSTIC_LOCATION_CODE,
                    "host language backend returned workspace diagnostics with missing, foreign, or invalid source locations; those diagnostics were suppressed",
                    source,
                    Span::new(0, source.text().len()),
                );
                if let Some(diagnostic) = diagnostic_for(source, &invalid_locations, self.encoding)
                {
                    diagnostics.push(diagnostic);
                }
            }

            let published = &mut diagnostics_by_uri
                .entry(uri_key)
                .or_insert_with(|| (uri, Vec::new()))
                .1;
            for diagnostic in diagnostics {
                if !published.contains(&diagnostic) {
                    published.push(diagnostic);
                }
            }
        }

        let mut current_uris = HashMap::new();
        let mut publications = diagnostics_by_uri
            .into_iter()
            .filter_map(|(uri_key, (uri, diagnostics))| {
                if diagnostics.is_empty() {
                    return None;
                }

                current_uris.insert(uri_key, uri.clone());
                Some((uri, diagnostics))
            })
            .collect::<Vec<_>>();
        publications.sort_by(|left, right| left.0.as_str().cmp(right.0.as_str()));

        for (uri, diagnostics) in publications {
            publish_diagnostics(connection, uri, None, diagnostics)?;
        }

        let mut stale_uris = self
            .workspace_diagnostic_uris
            .iter()
            .filter(|(uri_key, _)| {
                !current_uris.contains_key(*uri_key) && !open_uris.contains(*uri_key)
            })
            .map(|(_, uri)| uri.clone())
            .collect::<Vec<_>>();
        stale_uris.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        for uri in stale_uris {
            publish_diagnostics(connection, uri, None, Vec::new())?;
        }

        self.workspace_diagnostic_uris = current_uris;
        Ok(())
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
