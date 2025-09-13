pub mod document;
pub mod edit;
pub mod position;
pub mod with_layers;
pub mod with_state;

// Re-export commonly used types
pub use document::TextDocument;
pub use with_layers::ParsedDocument;
pub use with_state::StatefulDocument;
