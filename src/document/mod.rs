pub mod injection_mapper;
pub mod store;

mod model;

// Re-export main types
pub use injection_mapper::{
    InjectionPositionMapper, contains_offset, doc_to_layer_offset, layer_to_doc_offset,
};
pub use model::Document;
pub use store::DocumentStore;
