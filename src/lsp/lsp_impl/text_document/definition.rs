//! Goto definition method for Kakehashi.

use tower_lsp_server::jsonrpc::{Id, Result};
use tower_lsp_server::ls_types::{
    GotoDefinitionParams, GotoDefinitionResponse, Location, LocationLink, MessageType,
};

use crate::language::InjectionResolver;
use crate::lsp::bridge::UpstreamId;
use crate::lsp::get_current_request_id;
use crate::text::PositionMapper;

use super::super::{Kakehashi, uri_to_url};

/// Convert LocationLink to Location for clients that don't support link format.
///
/// Uses `target_selection_range` (the symbol name) rather than `target_range`
/// (the whole definition) for more precise navigation to the symbol itself.
fn location_link_to_location(link: LocationLink) -> Location {
    Location {
        uri: link.target_uri,
        range: link.target_selection_range,
    }
}

impl Kakehashi {
    pub(crate) async fn goto_definition_impl(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let lsp_uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        // Convert ls_types::Uri to url::Url for internal use
        let Ok(uri) = uri_to_url(&lsp_uri) else {
            log::warn!("Invalid URI in gotoDefinition: {}", lsp_uri.as_str());
            return Ok(None);
        };

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "goto_definition called for {} at line {} col {}",
                    uri, position.line, position.character
                ),
            )
            .await;

        // Get document snapshot (minimizes lock duration)
        let (snapshot, missing_message) = match self.documents.get(&uri) {
            None => (None, Some("No document found")),
            Some(doc) => match doc.snapshot() {
                None => (None, Some("Document not fully initialized")),
                Some(snapshot) => (Some(snapshot), None),
            },
            // doc automatically dropped here, lock released
        };
        if let Some(message) = missing_message {
            self.client.log_message(MessageType::INFO, message).await;
            return Ok(None);
        }
        let snapshot = snapshot.expect("snapshot set when missing_message is None");

        // Get the language for this document
        let Some(language_name) = self.get_language_for_document(&uri) else {
            log::debug!(target: "kakehashi::definition", "No language detected");
            return Ok(None);
        };

        // Get injection query to detect injection regions
        let Some(injection_query) = self.language.get_injection_query(&language_name) else {
            return Ok(None);
        };

        // Resolve injection region at position (centralizes 29-86 lines of duplication)
        let mapper = PositionMapper::new(snapshot.text());
        let Some(byte_offset) = mapper.position_to_byte(position) else {
            return Ok(None);
        };

        let Some(resolved) = InjectionResolver::resolve_at_byte_offset(
            &self.language,
            self.bridge.region_id_tracker(),
            &uri,
            snapshot.tree(),
            snapshot.text(),
            injection_query.as_ref(),
            byte_offset,
        ) else {
            // Not in an injection region - return None
            return Ok(None);
        };

        // Get bridge server config for this language
        // The bridge filter is checked inside get_bridge_config_for_language
        let Some(resolved_config) =
            self.get_bridge_config_for_language(&language_name, &resolved.injection_language)
        else {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!(
                        "No bridge server configured for language: {} (host: {})",
                        resolved.injection_language, language_name
                    ),
                )
                .await;
            return Ok(None);
        };

        // Send definition request via language server pool
        // Get upstream request ID from task-local storage (set by RequestIdCapture middleware)
        let upstream_request_id = match get_current_request_id() {
            Some(Id::Number(n)) => UpstreamId::Number(n),
            Some(Id::String(s)) => UpstreamId::String(s),
            // For notifications without ID or null ID, use Null to avoid collision with ID 0
            None | Some(Id::Null) => UpstreamId::Null,
        };
        let response = self
            .bridge
            .pool()
            .send_definition_request(
                &resolved_config.server_name,
                &resolved_config.config,
                &uri,
                position,
                &resolved.injection_language,
                &resolved.region.region_id,
                resolved.region.line_range.start,
                &resolved.virtual_content,
                upstream_request_id,
            )
            .await;

        match response {
            Ok(Some(links)) => {
                if self.supports_definition_link() {
                    Ok(Some(GotoDefinitionResponse::Link(links)))
                } else {
                    let locations: Vec<Location> =
                        links.into_iter().map(location_link_to_location).collect();
                    Ok(Some(GotoDefinitionResponse::Array(locations)))
                }
            }
            Ok(None) => Ok(None),
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Bridge definition request failed: {}", e),
                    )
                    .await;
                Ok(None)
            }
        }
    }
}
