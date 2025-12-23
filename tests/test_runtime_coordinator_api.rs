use treesitter_ls::language::LanguageCoordinator;

#[test]
fn coordinator_should_resolve_filetype() {
    let coordinator = LanguageCoordinator::new();
    let _lang = coordinator.get_language_for_extension("rs");
}

#[test]
fn coordinator_should_expose_query_state_checks() {
    let coordinator = LanguageCoordinator::new();
    let _has_queries: bool = coordinator.has_queries("rust");
}

#[test]
fn coordinator_should_expose_highlight_queries() {
    let coordinator = LanguageCoordinator::new();
    let _query = coordinator.get_highlight_query("rust");
}

#[test]
fn coordinator_should_expose_locals_queries() {
    let coordinator = LanguageCoordinator::new();
    let _query = coordinator.get_locals_query("rust");
}

#[test]
fn coordinator_should_provide_capture_mappings() {
    let coordinator = LanguageCoordinator::new();
    let _mappings = coordinator.get_capture_mappings();
}

#[test]
fn test_coordinator_has_parser_available() {
    let coordinator = LanguageCoordinator::new();

    // No languages loaded initially - should return false
    assert!(!coordinator.has_parser_available("rust"));

    // This test verifies the API is exposed on LanguageCoordinator.
    // The full behavior (true when loaded) is tested in unit tests
    // via register_language_for_test which is only available there.
}
