pub mod injection_mapper;
pub mod layer;
pub mod layer_manager;
pub mod store;
pub mod view;

mod model;

// Re-export main types
pub use injection_mapper::{
    InjectionPositionMapper, contains_offset, doc_to_layer_offset, layer_to_doc_offset,
};
pub use layer::LanguageLayer;
pub use layer_manager::LayerManager;
pub use model::{Document, SemanticSnapshot};
pub use store::{DocumentHandle, DocumentStore};
pub use view::DocumentView;
