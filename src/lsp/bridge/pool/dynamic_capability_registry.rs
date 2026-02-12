use std::collections::HashMap;
use std::sync::RwLock;

use log::warn;
use tower_lsp_server::ls_types::{Registration, Unregistration};

/// Thread-safe store for dynamically registered LSP capabilities.
///
/// Downstream language servers (e.g., Pyright) register capabilities dynamically
/// via `client/registerCapability` after the initialize handshake. This registry
/// tracks those registrations so the bridge can check capability support.
///
/// # Simplification
///
/// The LSP spec allows multiple registrations per method (with different document
/// selectors and IDs). We key by method name only, so later registrations for the
/// same method overwrite earlier ones. This is sufficient for our use case
/// (`has_registration` check) but means we don't track per-selector registrations.
pub(crate) struct DynamicCapabilityRegistry {
    registrations: RwLock<HashMap<String, Registration>>,
}

impl DynamicCapabilityRegistry {
    pub(crate) fn new() -> Self {
        Self {
            registrations: RwLock::new(HashMap::new()),
        }
    }

    pub(crate) fn register(&self, registrations: Vec<Registration>) {
        let mut guard = match self.registrations.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!(
                    target: "kakehashi::lock_recovery",
                    "Recovered from poisoned lock in DynamicCapabilityRegistry::register()"
                );
                poisoned.into_inner()
            }
        };
        for reg in registrations {
            guard.insert(reg.method.clone(), reg);
        }
    }

    pub(crate) fn unregister(&self, unregistrations: Vec<Unregistration>) {
        let mut guard = match self.registrations.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!(
                    target: "kakehashi::lock_recovery",
                    "Recovered from poisoned lock in DynamicCapabilityRegistry::unregister()"
                );
                poisoned.into_inner()
            }
        };
        for unreg in unregistrations {
            guard.remove(&unreg.method);
        }
    }

    #[allow(dead_code)] // Will be used by unified capability check in a subsequent subtask
    pub(crate) fn has_registration(&self, method: &str) -> bool {
        let guard = match self.registrations.read() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!(
                    target: "kakehashi::lock_recovery",
                    "Recovered from poisoned lock in DynamicCapabilityRegistry::has_registration()"
                );
                poisoned.into_inner()
            }
        };
        guard.contains_key(method)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use tower_lsp_server::ls_types::{Registration, Unregistration};

    use super::DynamicCapabilityRegistry;

    fn make_registration(id: &str, method: &str) -> Registration {
        Registration {
            id: id.to_string(),
            method: method.to_string(),
            register_options: None,
        }
    }

    fn make_unregistration(id: &str, method: &str) -> Unregistration {
        Unregistration {
            id: id.to_string(),
            method: method.to_string(),
        }
    }

    #[test]
    fn register_stores_capability() {
        let registry = DynamicCapabilityRegistry::new();
        let reg = make_registration("1", "textDocument/completion");

        registry.register(vec![reg]);

        assert!(registry.has_registration("textDocument/completion"));
    }

    #[test]
    fn unregister_removes_capability() {
        let registry = DynamicCapabilityRegistry::new();
        let reg = make_registration("1", "textDocument/completion");
        registry.register(vec![reg]);

        let unreg = make_unregistration("1", "textDocument/completion");
        registry.unregister(vec![unreg]);

        assert!(!registry.has_registration("textDocument/completion"));
    }

    #[test]
    fn has_registration_returns_false_for_unknown() {
        let registry = DynamicCapabilityRegistry::new();

        assert!(!registry.has_registration("textDocument/hover"));
    }

    #[test]
    fn register_overwrites_same_method() {
        let registry = DynamicCapabilityRegistry::new();
        let reg1 = make_registration("1", "textDocument/completion");
        let reg2 = make_registration("2", "textDocument/completion");

        registry.register(vec![reg1]);
        registry.register(vec![reg2]);

        assert!(registry.has_registration("textDocument/completion"));
        // Verify the latest registration is stored (id "2", not "1")
        let guard = registry.registrations.read().unwrap();
        let stored = guard.get("textDocument/completion").unwrap();
        assert_eq!(stored.id, "2");
    }

    #[test]
    fn poison_recovery_on_read() {
        let registry = Arc::new(DynamicCapabilityRegistry::new());
        let reg = make_registration("1", "textDocument/completion");
        registry.register(vec![reg]);

        // Poison the RwLock by panicking while holding a write guard
        let registry_clone = Arc::clone(&registry);
        let handle = thread::spawn(move || {
            let _guard = registry_clone.registrations.write().unwrap();
            panic!("intentional panic to poison the lock");
        });
        let _ = handle.join(); // Wait for thread to finish (it panicked)

        // Verify the lock is poisoned
        assert!(registry.registrations.read().is_err());

        // has_registration should recover from the poisoned lock
        assert!(registry.has_registration("textDocument/completion"));
    }

    #[test]
    fn poison_recovery_on_write() {
        let registry = Arc::new(DynamicCapabilityRegistry::new());

        // Poison the RwLock by panicking while holding a write guard
        let registry_clone = Arc::clone(&registry);
        let handle = thread::spawn(move || {
            let _guard = registry_clone.registrations.write().unwrap();
            panic!("intentional panic to poison the lock");
        });
        let _ = handle.join(); // Wait for thread to finish (it panicked)

        // Verify the lock is poisoned
        assert!(registry.registrations.write().is_err());

        // register should recover from the poisoned lock
        let reg = make_registration("1", "textDocument/hover");
        registry.register(vec![reg]);

        // Verify the registration was stored despite the poisoned lock
        assert!(registry.has_registration("textDocument/hover"));
    }
}
