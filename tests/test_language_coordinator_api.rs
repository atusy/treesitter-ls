use std::collections::HashMap;
use treesitter_ls::language::LanguageCoordinator;

#[test]
fn test_language_coordinator_should_provide_filetype_map_access() {
    let coordinator = LanguageCoordinator::new();

    // get_filetype_map メソッドが存在し、HashMap を返すことを確認
    let _map: HashMap<String, String> = coordinator.get_filetype_map();
}

#[test]
fn test_language_coordinator_should_check_if_queries_exist() {
    let coordinator = LanguageCoordinator::new();

    // has_queries メソッドが存在し、bool を返すことを確認
    let _has_queries: bool = coordinator.has_queries("rust");
}

#[test]
fn test_language_coordinator_should_get_highlight_query() {
    let coordinator = LanguageCoordinator::new();

    // get_highlight_query メソッドが存在し、Option<Arc<Query>> を返すことを確認
    let _query = coordinator.get_highlight_query("rust");
}

#[test]
fn test_language_coordinator_should_get_locals_query() {
    let coordinator = LanguageCoordinator::new();

    // get_locals_query メソッドが存在し、Option<Arc<Query>> を返すことを確認
    let _query = coordinator.get_locals_query("rust");
}

#[test]
fn test_language_coordinator_should_get_capture_mappings() {
    let coordinator = LanguageCoordinator::new();

    // get_capture_mappings メソッドが存在し、CaptureMappings を返すことを確認
    let _mappings = coordinator.get_capture_mappings();
}
