//! Goto definition method for Kakehashi.

use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
    GotoDefinitionParams, GotoDefinitionResponse, Location, MessageType,
};

use crate::lsp::bridge::location_link_to_location;

use super::super::Kakehashi;

impl Kakehashi {
    pub(crate) async fn goto_definition_impl(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let lsp_uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let Some(ctx) = self
            .resolve_bridge_context(&lsp_uri, position, "goto_definition")
            .await
        else {
            return Ok(None);
        };

        // Send definition request via language server pool
        let response = self
            .bridge
            .pool()
            .send_definition_request(
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
