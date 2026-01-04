pub mod auto_install;

// Public for E2E tests (feature gated)
#[cfg(feature = "e2e")]
pub mod bridge;
#[cfg(not(feature = "e2e"))]
pub(crate) mod bridge;

mod lsp_impl;
mod progress;
mod semantic_request_tracker;
mod settings;

pub use lsp_impl::TreeSitterLs;
pub use settings::{
    SettingsEvent, SettingsEventKind, SettingsLoadOutcome, SettingsSource, load_settings,
};
