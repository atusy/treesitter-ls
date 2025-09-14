pub mod languages;

// Re-export DocumentStore from document module
pub use crate::document::DocumentStore;

// Create documents module re-export for backward compatibility
pub mod documents {
    pub use crate::document::store::{Document, DocumentStore};
}
