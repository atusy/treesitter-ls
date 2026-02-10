//! Rename method for Kakehashi.

use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{MessageType, RenameParams, WorkspaceEdit};

use super::super::Kakehashi;

impl Kakehashi {
    pub(crate) async fn rename_impl(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let lsp_uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;

        let Some(ctx) = self
            .resolve_bridge_context(&lsp_uri, position, "rename")
            .await
        else {
            return Ok(None);
        };

        // Send rename request via language server pool
        let response = self
            .bridge
            .pool()
            .send_rename_request(
                &ctx.resolved_config.server_name,
                &ctx.resolved_config.config,
                &ctx.uri,
                ctx.position,
                &ctx.resolved.injection_language,
                &ctx.resolved.region.region_id,
                ctx.resolved.region.line_range.start,
                &ctx.resolved.virtual_content,
                &new_name,
                ctx.upstream_request_id,
            )
            .await;

        match response {
            Ok(workspace_edit) => Ok(workspace_edit),
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Bridge rename request failed: {}", e),
                    )
                    .await;
                Ok(None)
            }
        }
    }
}
