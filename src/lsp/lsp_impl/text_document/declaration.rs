//! Goto declaration method for Kakehashi.

use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::request::{GotoDeclarationParams, GotoDeclarationResponse};
use tower_lsp_server::ls_types::{Location, MessageType};

use crate::lsp::bridge::location_link_to_location;

use super::super::Kakehashi;

impl Kakehashi {
    pub(crate) async fn goto_declaration_impl(
        &self,
        params: GotoDeclarationParams,
    ) -> Result<Option<GotoDeclarationResponse>> {
        let lsp_uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let Some(ctx) = self
            .resolve_bridge_context(&lsp_uri, position, "goto_declaration")
            .await
        else {
            return Ok(None);
        };

        // Send declaration request via language server pool
        let response = self
            .bridge
            .pool()
            .send_declaration_request(
                &ctx.resolved_config.server_name,
                &ctx.resolved_config.config,
                &ctx.uri,
                ctx.position,
                &ctx.resolved.injection_language,
                &ctx.resolved.region.region_id,
                ctx.resolved.region.line_range.start,
                &ctx.resolved.virtual_content,
                ctx.upstream_request_id,
            )
            .await;

        match response {
            Ok(Some(links)) => {
                if self.supports_declaration_link() {
                    Ok(Some(GotoDeclarationResponse::Link(links)))
                } else {
                    let locations: Vec<Location> =
                        links.into_iter().map(location_link_to_location).collect();
                    Ok(Some(GotoDeclarationResponse::Array(locations)))
                }
            }
            Ok(None) => Ok(None),
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Bridge declaration request failed: {}", e),
                    )
                    .await;
                Ok(None)
            }
        }
    }
}
