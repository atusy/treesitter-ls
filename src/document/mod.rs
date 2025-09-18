pub mod store;
pub mod view;

mod model;

// Re-export main types
pub use model::{Document, SemanticSnapshot};
pub use store::{DocumentHandle, DocumentStore};
pub use view::DocumentView;
