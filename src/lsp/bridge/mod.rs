//! LSP Bridge infrastructure for async language server communication
//!
//! This module provides the bridge infrastructure for communicating with
//! language servers asynchronously for injection regions.

// Public for E2E tests (feature gated)
#[cfg(feature = "e2e")]
pub mod connection;
#[cfg(not(feature = "e2e"))]
pub(crate) mod connection;

pub(crate) mod pool;

#[allow(unused_imports)] // Used in Phase 2 (real LSP communication)
pub(crate) use connection::BridgeConnection;
pub(crate) use pool::LanguageServerPool;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_structure_exists() {
        // This test verifies that the module structure is correctly set up
        // and that we can reference the key types
        let _type_check: Option<BridgeConnection> = None;
        let _pool_check: Option<LanguageServerPool> = None;
    }
}
