//! Moniker method for Kakehashi.

use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{MessageType, Moniker, MonikerParams};

use super::super::Kakehashi;

impl Kakehashi {
    pub(crate) async fn moniker_impl(&self, params: MonikerParams) -> Result<Option<Vec<Moniker>>> {
        let lsp_uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let Some(ctx) = self
            .resolve_bridge_context(&lsp_uri, position, "moniker")
            .await
        else {
            return Ok(None);
        };

        // Send moniker request via language server pool
        let response = self
            .bridge
            .pool()
            .send_moniker_request(
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
            Ok(monikers) => Ok(monikers),
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Bridge moniker request failed: {}", e),
                    )
                    .await;
                Ok(None)
            }
        }
    }
}
