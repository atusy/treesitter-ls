pub mod layer;
pub mod layer_manager;
pub mod store;
pub mod view;

mod model;

// Re-export main types
pub use layer::LanguageLayer;
pub use layer_manager::LayerManager;
pub use model::{Document, SemanticSnapshot};
pub use store::{DocumentHandle, DocumentStore};
pub use view::DocumentView;
