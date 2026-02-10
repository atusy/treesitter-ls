//! Color presentation method for Kakehashi.

use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{ColorPresentation, ColorPresentationParams, MessageType};

use super::super::Kakehashi;

impl Kakehashi {
    pub(crate) async fn color_presentation_impl(
        &self,
        params: ColorPresentationParams,
    ) -> Result<Vec<ColorPresentation>> {
        let lsp_uri = params.text_document.uri;
        let range = params.range;
        let color = params.color;

        // Use resolve_bridge_context() to handle injection resolution via range.start
        let Some(ctx) = self
            .resolve_bridge_context(&lsp_uri, range.start, "colorPresentation")
            .await
        else {
            return Ok(Vec::new());
        };

        // Convert Color to JSON Value for bridge
        let color_json = serde_json::to_value(color).unwrap_or_default();

        // Send color presentation request via language server pool
        let response = self
            .bridge
            .pool()
            .send_color_presentation_request(
                &ctx.resolved_config.server_name,
                &ctx.resolved_config.config,
                &ctx.uri,
                range,
                &color_json,
                &ctx.resolved.injection_language,
                &ctx.resolved.region.region_id,
                ctx.resolved.region.line_range.start,
                &ctx.resolved.virtual_content,
                ctx.upstream_request_id,
            )
            .await;

        match response {
            Ok(presentations) => Ok(presentations),
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Bridge color presentation request failed: {}", e),
                    )
                    .await;
                Ok(Vec::new())
            }
        }
    }
}
