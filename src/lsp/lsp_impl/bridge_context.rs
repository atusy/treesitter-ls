//! Shared preamble for bridge endpoint implementations.
//!
//! All bridge endpoints (definition, type_definition, implementation, declaration,
//! references) follow the same pattern of resolving injection context before sending
//! requests. This module extracts that shared preamble into a single method.

use tower_lsp_server::jsonrpc::Id;
use tower_lsp_server::ls_types::{MessageType, Position, Uri};
use url::Url;

use crate::language::injection::ResolvedInjection;
use crate::lsp::bridge::{ResolvedServerConfig, UpstreamId};
use crate::lsp::get_current_request_id;
use crate::text::PositionMapper;

use super::{Kakehashi, uri_to_url};

/// All resolved context needed to send a bridge request.
///
/// Produced by `Kakehashi::resolve_bridge_context`, which handles all the
/// common preamble steps: URI conversion, document snapshot, language detection,
/// injection resolution, bridge config lookup, and upstream request ID extraction.
pub(crate) struct BridgeRequestContext {
    /// The parsed document URL (url::Url).
    pub(crate) uri: Url,
    /// The cursor position within the document.
    pub(crate) position: Position,
    /// The resolved injection region with virtual content and region metadata.
    pub(crate) resolved: ResolvedInjection,
    /// The bridge server config (server name + spawn config).
    pub(crate) resolved_config: ResolvedServerConfig,
    /// The upstream JSON-RPC request ID for cancel forwarding.
    pub(crate) upstream_request_id: UpstreamId,
}

impl Kakehashi {
    /// Resolve injection context for a bridge endpoint request.
    ///
    /// This method encapsulates the shared preamble across all bridge endpoints
    /// (definition, type_definition, implementation, declaration, references):
    ///
    /// 1. Converts URI from ls_types to url::Url
    /// 2. Logs the method invocation
    /// 3. Gets document snapshot
    /// 4. Detects document language
    /// 5. Gets injection query
    /// 6. Resolves injection region at position
    /// 7. Looks up bridge server config
    /// 8. Extracts upstream request ID from task-local storage
    ///
    /// Returns `None` for any early-exit condition (invalid URI, no document,
    /// no language, no injection at position, no bridge config).
    ///
    /// # Arguments
    /// * `lsp_uri` - The document URI from the LSP params
    /// * `position` - The cursor position
    /// * `method_name` - Name for log messages (e.g., "goto_definition", "references")
    pub(crate) async fn resolve_bridge_context(
        &self,
        lsp_uri: &Uri,
        position: Position,
        method_name: &str,
    ) -> Option<BridgeRequestContext> {
        // Convert ls_types::Uri to url::Url for internal use
        let Ok(uri) = uri_to_url(lsp_uri) else {
            log::warn!("Invalid URI in {}: {}", method_name, lsp_uri.as_str());
            return None;
        };

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "{} called for {} at line {} col {}",
                    method_name, uri, position.line, position.character
                ),
            )
            .await;

        // Get document snapshot (minimizes lock duration)
        let snapshot = match self.documents.get(&uri) {
            None => {
                self.client
                    .log_message(MessageType::INFO, "No document found")
                    .await;
                return None;
            }
            Some(doc) => match doc.snapshot() {
                None => {
                    self.client
                        .log_message(MessageType::INFO, "Document not fully initialized")
                        .await;
                    return None;
                }
                Some(snapshot) => snapshot,
            },
            // doc automatically dropped here, lock released
        };

        // Get the language for this document
        let Some(language_name) = self.get_language_for_document(&uri) else {
            log::debug!("kakehashi::{}: No language detected", method_name);
            return None;
        };

        // Get injection query to detect injection regions
        let injection_query = self.language.get_injection_query(&language_name)?;

        // Resolve injection region at position
        let mapper = PositionMapper::new(snapshot.text());
        let byte_offset = mapper.position_to_byte(position)?;

        let Some(resolved) = crate::language::InjectionResolver::resolve_at_byte_offset(
            &self.language,
            self.bridge.region_id_tracker(),
            &uri,
            snapshot.tree(),
            snapshot.text(),
            injection_query.as_ref(),
            byte_offset,
        ) else {
            // Not in an injection region - return None
            return None;
        };

        // Get bridge server config for this language
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
            return None;
        };

        // Get upstream request ID from task-local storage (set by RequestIdCapture middleware)
        let upstream_request_id = match get_current_request_id() {
            Some(Id::Number(n)) => UpstreamId::Number(n),
            Some(Id::String(s)) => UpstreamId::String(s),
            // For notifications without ID or null ID, use Null to avoid collision with ID 0
            None | Some(Id::Null) => UpstreamId::Null,
        };

        Some(BridgeRequestContext {
            uri,
            position,
            resolved,
            resolved_config,
            upstream_request_id,
        })
    }
}
