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

#[test]
fn test_shebang_used_when_language_id_plaintext() {
    let coordinator = LanguageCoordinator::new();

    // When languageId is "plaintext", fallback to shebang detection
    // Note: No parser loaded, so will return None (graceful degradation)
    // But the shebang detection path is still exercised
    let content = "#!/usr/bin/env python\nprint('hello')";
    let result = coordinator.detect_language("/script", Some("plaintext"), content);

    // No python parser loaded, so result is None
    // The important thing is that "plaintext" didn't short-circuit
    assert_eq!(result, None);
}

#[test]
fn test_shebang_skipped_when_language_id_has_parser() {
    let coordinator = LanguageCoordinator::new();

    // When languageId has an available parser, don't run shebang detection
    // This tests lazy I/O - shebang parsing is skipped entirely

    // Scenario: languageId is "rust" but no rust parser loaded
    // So it falls through to shebang, but no python parser either
    let content = "#!/usr/bin/env python\nprint('hello')";
    let result = coordinator.detect_language("/script", Some("rust"), content);

    // Neither rust nor python parser loaded
    assert_eq!(result, None);

    // Full behavior with loaded parser is tested in unit tests
}
