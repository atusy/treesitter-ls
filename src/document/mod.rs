pub mod coordinates;
pub mod edits;
pub mod layer;
pub mod layer_manager;
pub mod store;

mod model;

// Re-export main types
pub use coordinates::{
    PositionMapper, SimplePositionMapper, InjectionPositionMapper,
    compute_line_starts, extract_text_from_ranges,
    doc_to_layer_offset, layer_to_doc_offset, contains_offset
};
pub use edits::{adjust_ranges_for_edit, edit_affects_ranges, transform_edit_for_injection};
pub use layer::LanguageLayer;
pub use layer_manager::LayerManager;
pub use model::Document;
pub use store::DocumentStore;
