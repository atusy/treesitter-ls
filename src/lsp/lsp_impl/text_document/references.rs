//! Find references method for Kakehashi.

use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{Location, MessageType, ReferenceParams};

use super::super::Kakehashi;

impl Kakehashi {
    pub(crate) async fn references_impl(
        &self,
        params: ReferenceParams,
    ) -> Result<Option<Vec<Location>>> {
        let lsp_uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;

        let Some(ctx) = self
            .resolve_bridge_context(&lsp_uri, position, "references")
            .await
        else {
            return Ok(None);
        };

        // Send references request via language server pool
        let response = self
            .bridge
            .pool()
            .send_references_request(
                &ctx.resolved_config.server_name,
                &ctx.resolved_config.config,
                &ctx.uri,
                ctx.position,
                &ctx.resolved.injection_language,
                &ctx.resolved.region.region_id,
                ctx.resolved.region.line_range.start,
                &ctx.resolved.virtual_content,
                include_declaration,
                ctx.upstream_request_id,
            )
            .await;

        match response {
            Ok(locations) => Ok(locations),
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Bridge references request failed: {}", e),
                    )
                    .await;
                Ok(None)
            }
        }
    }
}
