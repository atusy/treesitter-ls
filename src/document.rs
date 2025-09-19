pub mod store;

mod model;

// Re-export main types
pub use model::Document;
pub use store::{DocumentHandle, DocumentStore};
