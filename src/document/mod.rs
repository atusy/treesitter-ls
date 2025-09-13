pub mod coordinates;

// Re-export for backward compatibility during migration
pub use coordinates::{PositionMapper, SimplePositionMapper, compute_line_starts};
