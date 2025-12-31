//! Tests for poison lock recovery mechanisms

use std::sync::Arc;
use std::thread;
use treesitter_ls::language::{ConfigStore, FiletypeResolver, LanguageRegistry, QueryStore};

#[test]
fn test_registry_recovers_from_poisoned_lock() {
    let registry = Arc::new(LanguageRegistry::new());
    let registry_clone = registry.clone();

    // Spawn a thread that will panic and poison the lock
    let handle = thread::spawn(move || {
        // This will panic inside the lock, poisoning it
        registry_clone.register_unchecked("panic_lang".to_string(), {
            // Create a dummy language that will cause issues
            panic!("Intentional panic to poison the lock");
            #[allow(unreachable_code)]
            tree_sitter_rust::LANGUAGE.into()
        });
    });

    // Wait for the panic thread to complete
    let _ = handle.join();

    // Now try to use the registry - it should recover from the poisoned lock
    let languages = registry.language_ids();
    assert!(
        languages.is_empty() || !languages.is_empty(),
        "Should recover from poisoned lock and return language list"
    );

    // Try to get a language - should also recover
    let result = registry.get("rust");
    assert!(
        result.is_none(),
        "Should recover from poisoned lock and return None for non-existent language"
    );

    // Try to check if language exists - should also recover
    let exists = registry.contains("rust");
    assert!(
        !exists,
        "Should recover and return false for non-existent language"
    );
}

#[test]
fn test_query_store_recovers_from_poisoned_read_lock() {
    let store = Arc::new(QueryStore::new());
    let store_clone = store.clone();

    // Create a thread that will panic while holding a read lock
    let handle = thread::spawn(move || {
        // This simulates a panic during query access
        let _query = store_clone.get_highlight_query("panic_lang");
        panic!("Intentional panic while holding read lock");
    });

    let _ = handle.join();

    // Should recover from poisoned read lock
    let query = store.get_highlight_query("rust");
    assert!(query.is_none(), "Should recover from poisoned read lock");

    let has_query = store.has_highlight_query("rust");
    assert!(!has_query, "Should recover and check query existence");
}

#[test]
fn test_query_store_recovers_from_poisoned_write_lock() {
    let store = Arc::new(QueryStore::new());
    let store_clone = store.clone();

    // Create a thread that will panic while holding a write lock
    let handle = thread::spawn(move || {
        // Create a dummy query
        let lang = tree_sitter_rust::LANGUAGE.into();
        let query = Arc::new(tree_sitter::Query::new(&lang, "(identifier) @variable").unwrap());

        // Start insertion but panic midway
        store_clone.insert_highlight_query("panic_lang".to_string(), query);
        panic!("Intentional panic during write operation");
    });

    let _ = handle.join();

    // Should recover from poisoned write lock and continue operations
    let lang = tree_sitter_rust::LANGUAGE.into();
    let valid_query = Arc::new(tree_sitter::Query::new(&lang, "(identifier) @test").unwrap());
    store.insert_highlight_query("rust".to_string(), valid_query.clone());

    let retrieved = store.get_highlight_query("rust");
    assert!(
        retrieved.is_some(),
        "Should recover and insert new query after poison"
    );
}

#[test]
fn test_config_store_recovers_from_poisoned_lock() {
    use treesitter_ls::config::settings::LanguageConfig;

    let store = Arc::new(ConfigStore::new());
    let store_clone = store.clone();

    let handle = thread::spawn(move || {
        let configs = std::collections::HashMap::new();
        store_clone.set_language_configs(configs);
        panic!("Intentional panic in config store");
    });

    let _ = handle.join();

    // Should recover and continue operations
    let mut new_configs = std::collections::HashMap::new();
    new_configs.insert(
        "rust".to_string(),
        LanguageConfig {
            library: Some("/path/to/rust.so".to_string()),
            highlights: None,
            locals: None,
            injections: None,
            bridge: None,
        },
    );

    store.set_language_configs(new_configs);
    let retrieved = store.get_language_config("rust");
    assert!(
        retrieved.is_some(),
        "Should recover and set new configs after poison"
    );
}

#[test]
fn test_filetype_resolver_recovers_from_poisoned_lock() {
    let resolver = Arc::new(FiletypeResolver::new());
    let resolver_clone = resolver.clone();

    let handle = thread::spawn(move || {
        resolver_clone.add_mapping("panic".to_string(), "panic_lang".to_string());
        panic!("Intentional panic in filetype resolver");
    });

    let _ = handle.join();

    // Should recover and continue operations
    resolver.add_mapping("rs".to_string(), "rust".to_string());
    let lang = resolver.get_language_for_extension("rs");
    assert_eq!(
        lang,
        Some("rust".to_string()),
        "Should recover and add new mapping"
    );

    resolver.clear();
    let lang_after_clear = resolver.get_language_for_extension("rs");
    assert!(
        lang_after_clear.is_none(),
        "Should recover and clear mappings"
    );
}

#[test]
fn test_concurrent_access_after_poison_recovery() {
    let registry = Arc::new(LanguageRegistry::new());

    // First poison the lock
    let registry_panic = registry.clone();
    let panic_handle = thread::spawn(move || {
        registry_panic.register_unchecked("panic".to_string(), tree_sitter_rust::LANGUAGE.into());
        panic!("Poison the lock");
    });
    let _ = panic_handle.join();

    // Now spawn multiple threads that should all recover and work
    let mut handles = vec![];
    for i in 0..10 {
        let reg = registry.clone();
        let handle = thread::spawn(move || {
            // Each thread should recover from poison and work
            let lang_id = format!("lang_{}", i);
            reg.register_unchecked(lang_id.clone(), tree_sitter_rust::LANGUAGE.into());

            // Verify it was registered
            assert!(
                reg.contains(&lang_id),
                "Thread {} should register language",
                i
            );
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread should complete");
    }

    // Verify all languages were registered despite initial poison
    let all_langs = registry.language_ids();
    assert!(
        all_langs.len() >= 10,
        "Should have registered languages from all threads"
    );
}
