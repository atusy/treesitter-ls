//! Signature help method for TreeSitterLs.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use super::super::TreeSitterLs;

impl TreeSitterLs {
    pub(crate) async fn signature_help_impl(
        &self,
        _params: SignatureHelpParams,
    ) -> Result<Option<SignatureHelp>> {
        // Signature help bridging not yet implemented
        Ok(None)
    }
}
