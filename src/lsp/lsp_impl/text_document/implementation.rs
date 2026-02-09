//! Goto implementation method for Kakehashi.

use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::request::{GotoImplementationParams, GotoImplementationResponse};
use tower_lsp_server::ls_types::{Location, MessageType};

use crate::lsp::bridge::location_link_to_location;

use super::super::Kakehashi;

impl Kakehashi {
    pub(crate) async fn goto_implementation_impl(
        &self,
        params: GotoImplementationParams,
    ) -> Result<Option<GotoImplementationResponse>> {
        let lsp_uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let Some(ctx) = self
            .resolve_bridge_context(&lsp_uri, position, "goto_implementation")
            .await
        else {
            return Ok(None);
        };

        // Send implementation request via language server pool
        let response = self
            .bridge
            .pool()
            .send_implementation_request(
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
                if self.supports_implementation_link() {
                    Ok(Some(GotoImplementationResponse::Link(links)))
                } else {
                    let locations: Vec<Location> =
                        links.into_iter().map(location_link_to_location).collect();
                    Ok(Some(GotoImplementationResponse::Array(locations)))
                }
            }
            Ok(None) => Ok(None),
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Bridge implementation request failed: {}", e),
                    )
                    .await;
                Ok(None)
            }
        }
    }
}
