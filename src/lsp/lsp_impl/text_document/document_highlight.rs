//! Document highlight method for Kakehashi.

use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{DocumentHighlight, DocumentHighlightParams, MessageType};

use super::super::Kakehashi;

impl Kakehashi {
    pub(crate) async fn document_highlight_impl(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let lsp_uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let Some(ctx) = self
            .resolve_bridge_context(&lsp_uri, position, "document_highlight")
            .await
        else {
            return Ok(None);
        };

        // Send document highlight request via language server pool
        let response = self
            .bridge
            .pool()
            .send_document_highlight_request(
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
            Ok(highlights) => Ok(highlights),
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Bridge document highlight request failed: {}", e),
                    )
                    .await;
                Ok(None)
            }
        }
    }
}
