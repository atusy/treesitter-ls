//! Opened virtual document tracking.
//!
//! This module provides a struct for tracking virtual documents that have been
//! opened via didOpen on downstream language servers.

use crate::lsp::bridge::protocol::VirtualDocumentUri;

/// Represents an opened virtual document for tracking.
///
/// Used for didClose propagation when host document closes.
/// Each OpenedVirtualDoc represents a virtual document that was opened
/// via didOpen on a downstream language server.
#[derive(Debug, Clone)]
pub(crate) struct OpenedVirtualDoc {
    /// The virtual document URI (contains language and region_id)
    pub(crate) virtual_uri: VirtualDocumentUri,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::bridge::pool::test_helpers::url_to_uri;
    use url::Url;

    /// Test that OpenedVirtualDoc struct has required fields.
    ///
    /// The struct should have:
    /// - virtual_uri: VirtualDocumentUri (typed URI with language and region_id)
    #[tokio::test]
    async fn opened_virtual_doc_struct_has_required_fields() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", "region-0");
        let doc = OpenedVirtualDoc {
            virtual_uri: virtual_uri.clone(),
        };

        assert_eq!(doc.virtual_uri.language(), "lua");
        assert_eq!(doc.virtual_uri.region_id(), "region-0");
    }
}
