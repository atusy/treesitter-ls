use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Default)]
pub struct WorkspaceState {
    root_path: Mutex<Option<PathBuf>>,
}

impl WorkspaceState {
    pub fn new() -> Self {
        Self {
            root_path: Mutex::new(None),
        }
    }

    pub fn set_root_path(&self, path: Option<PathBuf>) {
        match self.root_path.lock() {
            Ok(mut guard) => *guard = path,
            Err(poisoned) => {
                log::warn!(
                    target: "treesitter_ls::lock_recovery",
                    "Recovered from poisoned lock in workspace::state::set_root_path",
                );
                *poisoned.into_inner() = path;
            }
        }
    }

    pub fn root_path(&self) -> Option<PathBuf> {
        match self.root_path.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => {
                log::warn!(
                    target: "treesitter_ls::lock_recovery",
                    "Recovered from poisoned lock in workspace::state::root_path",
                );
                poisoned.into_inner().clone()
            }
        }
    }
}
