use std::collections::HashMap;

use super::super::{LanguageBackend, ResolvedLocationSource, ServerState};
use crate::conversion::{byte_offset, lsp_location, to_lsp_range};
use lsp_types::{
    GotoDefinitionParams, Location, ReferenceParams, RenameParams, SymbolInformation, TextEdit,
    WorkspaceEdit,
};

impl<B: LanguageBackend> ServerState<B> {
    pub(super) fn definition(&self, params: GotoDefinitionParams) -> Option<Vec<Location>> {
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;
        let document = self.documents.get(&uri)?;
        let source = self.sources.get(document.source_id)?;
        if !document.analysis.matches_source(source) {
            return Some(Vec::new());
        }
        let offset = byte_offset(source, position, self.encoding)?;
        Some(
            self.backend
                .definition(source, &document.analysis, offset)
                .iter()
                .filter_map(|location| self.lsp_location(location))
                .collect(),
        )
    }

    pub(super) fn references(&self, params: ReferenceParams) -> Option<Vec<Location>> {
        let position = params.text_document_position.position;
        let uri = params.text_document_position.text_document.uri;
        let document = self.documents.get(&uri)?;
        let source = self.sources.get(document.source_id)?;
        if !document.analysis.matches_source(source) {
            return Some(Vec::new());
        }
        let offset = byte_offset(source, position, self.encoding)?;
        Some(
            self.backend
                .references(
                    source,
                    &document.analysis,
                    offset,
                    params.context.include_declaration,
                )
                .iter()
                .filter_map(|location| self.lsp_location(location))
                .collect(),
        )
    }

    pub(super) fn rename(&self, params: RenameParams) -> Option<WorkspaceEdit> {
        let position = params.text_document_position.position;
        let uri = params.text_document_position.text_document.uri;
        let document = self.documents.get(&uri)?;
        let source = self.sources.get(document.source_id)?;
        if !document.analysis.matches_source(source) {
            return None;
        }
        let offset = byte_offset(source, position, self.encoding)?;
        let locations =
            self.backend
                .rename(source, &document.analysis, offset, &params.new_name)?;
        if locations.is_empty() {
            return None;
        }
        let mut edits_by_uri: HashMap<String, Vec<(langmeld_core::Span, TextEdit)>> =
            HashMap::new();
        for location in locations {
            let target = self.resolve_location_source(&location)?;
            let target_uri = target.uri();
            let target_source = target.source();
            let span = location.span();
            if span.is_empty() {
                return None;
            }
            let range = to_lsp_range(target_source, span, self.encoding)?;
            edits_by_uri
                .entry(target_uri.to_string())
                .or_default()
                .push((
                    span,
                    TextEdit {
                        range,
                        new_text: params.new_name.clone(),
                    },
                ));
        }

        #[expect(
            clippy::mutable_key_type,
            reason = "WorkspaceEdit requires URI keys, whose equality remains stable here"
        )]
        let mut changes = HashMap::with_capacity(edits_by_uri.len());
        for (uri, mut edits) in edits_by_uri {
            edits.sort_by_key(|(span, _)| (span.start, span.end));
            if edits.windows(2).any(|pair| pair[0].0.end > pair[1].0.start) {
                return None;
            }
            changes.insert(
                uri.parse().ok()?,
                edits.into_iter().map(|(_, edit)| edit).collect(),
            );
        }
        Some(WorkspaceEdit {
            changes: Some(changes),
            ..WorkspaceEdit::default()
        })
    }

    pub(super) fn workspace_symbols(&self, query: &str) -> Vec<SymbolInformation> {
        self.backend
            .workspace_symbols(query)
            .into_iter()
            .filter_map(|symbol| {
                if symbol.name.is_empty() {
                    return None;
                }
                let location = self.lsp_location(&symbol.location)?;
                #[expect(
                    deprecated,
                    reason = "lsp-types 0.97 requires the deprecated field during construction"
                )]
                Some(SymbolInformation {
                    name: symbol.name,
                    kind: symbol.kind.into(),
                    tags: None,
                    deprecated: None,
                    location,
                    container_name: symbol.container_name,
                })
            })
            .collect()
    }

    fn lsp_location(&self, location: &langmeld_core::SourceLocation) -> Option<Location> {
        let target = self.resolve_location_source(location)?;
        lsp_location(
            location,
            target.uri().clone(),
            target.source(),
            self.encoding,
        )
    }

    fn resolve_location_source<'a>(
        &'a self,
        location: &langmeld_core::SourceLocation,
    ) -> Option<ResolvedLocationSource<'a>> {
        if let Some((uri, source)) = self.documents.iter().find_map(|(uri, document)| {
            let source = self.sources.get(document.source_id)?;
            location.is_valid_for(source).then_some((uri, source))
        }) {
            return Some(ResolvedLocationSource::Open { uri, source });
        }

        let target = self.backend.source_for_location(location)?;
        location
            .is_valid_for(target.source())
            .then_some(ResolvedLocationSource::External(target))
    }
}
