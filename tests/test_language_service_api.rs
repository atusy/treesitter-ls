use treesitter_ls::workspace::languages::LanguageService;
use std::collections::HashMap;

#[test]
fn test_language_service_should_provide_filetype_map_access() {
    let service = LanguageService::new();

    // get_filetype_map メソッドが存在し、HashMap を返すことを確認
    let _map: HashMap<String, String> = service.get_filetype_map();
}

#[test]
fn test_language_service_should_check_if_queries_exist() {
    let service = LanguageService::new();

    // has_queries メソッドが存在し、bool を返すことを確認
    let _has_queries: bool = service.has_queries("rust");
}

#[test]
fn test_language_service_should_get_highlight_query() {
    // Red: get_highlight_query メソッドが存在しない
    let service = LanguageService::new();

    // get_highlight_query メソッドが存在し、Option<Query> を返すことを確認
    let _query = service.get_highlight_query("rust");
}