// TDD Edge Case Tests for Definition Jump
use tower_lsp::lsp_types::*;
use tower_lsp::{LanguageServer, LspService};
use treesitter_ls::*;
use serde_json;

mod edge_case_tests {
    use super::*;

    #[tokio::test]
    async fn test_variable_shadowing_should_prefer_local() {
        // RED: This test should FAIL initially - current implementation goes to import
        let (service, _socket) = LspService::new(TreeSitterLs::new);
        let server = service.inner();
        
        let init_params = InitializeParams {
            capabilities: ClientCapabilities::default(),
            initialization_options: Some(serde_json::json!({
                "languages": {
                    "rust": {
                        "filetypes": ["rs"],
                        "highlight": [{"path": "queries/rust/highlights.scm"}],
                        "locals": [{"path": "queries/rust/locals.scm"}]
                    }
                }
            })),
            ..Default::default()
        };
        
        let _ = server.initialize(init_params).await.unwrap();
        
        let uri = Url::parse("file:///shadowing_test.rs").unwrap();
        let content = r#"use tokio::io::stdin;

fn main() {
    let stdin = stdin();
    println!("{:?}", stdin);
}"#;
        
        server.did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "rust".to_string(),
                version: 1,
                text: content.to_string(),
            },
        }).await;
        
        // Jump from the reference in println! (line 4, should be around char 21)
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position { line: 4, character: 21 }, // "stdin" in println!
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        
        let result = server.goto_definition(params).await.unwrap();
        
        // Should go to local variable at line 3, NOT import at line 0
        match result {
            Some(GotoDefinitionResponse::Scalar(location)) => {
                assert_eq!(location.range.start.line, 3, 
                    "EXPECTED FAILURE: Should jump to local variable (line 3), not import (line 0). Got line {}",
                    location.range.start.line);
            }
            _ => panic!("Expected a definition location"),
        }
    }

    #[tokio::test] 
    async fn test_nested_scope_inner_shadows_outer() {
        // RED: This test should FAIL initially - current implementation might not handle nested scopes correctly
        let (service, _socket) = LspService::new(TreeSitterLs::new);
        let server = service.inner();
        
        let init_params = InitializeParams {
            capabilities: ClientCapabilities::default(),
            initialization_options: Some(serde_json::json!({
                "languages": {
                    "rust": {
                        "filetypes": ["rs"],
                        "highlight": [{"path": "queries/rust/highlights.scm"}],
                        "locals": [{"path": "queries/rust/locals.scm"}]
                    }
                }
            })),
            ..Default::default()
        };
        
        let _ = server.initialize(init_params).await.unwrap();
        
        let uri = Url::parse("file:///nested_scope_test.rs").unwrap();
        let content = r#"fn main() {
    let x = 1;
    {
        let x = 2;
        println!("{}", x);
    }
}"#;
        
        server.did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "rust".to_string(),
                version: 1,
                text: content.to_string(),
            },
        }).await;
        
        // Jump from the reference in inner println! (line 4)
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position { line: 4, character: 19 }, // "x" in println!
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        
        let result = server.goto_definition(params).await.unwrap();
        
        // Should go to inner scope variable at line 3, NOT outer scope at line 1
        match result {
            Some(GotoDefinitionResponse::Scalar(location)) => {
                assert_eq!(location.range.start.line, 3, 
                    "EXPECTED FAILURE: Should jump to inner scope variable (line 3), not outer scope (line 1). Got line {}",
                    location.range.start.line);
            }
            _ => panic!("Expected a definition location"),
        }
    }

    #[tokio::test]
    async fn test_function_call_vs_variable_context() {
        // RED: This test should FAIL initially - current implementation treats all identifiers the same
        let (service, _socket) = LspService::new(TreeSitterLs::new);
        let server = service.inner();
        
        let init_params = InitializeParams {
            capabilities: ClientCapabilities::default(),
            initialization_options: Some(serde_json::json!({
                "languages": {
                    "rust": {
                        "filetypes": ["rs"],
                        "highlight": [{"path": "queries/rust/highlights.scm"}],
                        "locals": [{"path": "queries/rust/locals.scm"}]
                    }
                }
            })),
            ..Default::default()
        };
        
        let _ = server.initialize(init_params).await.unwrap();
        
        let uri = Url::parse("file:///function_vs_variable_test.rs").unwrap();
        let content = r#"fn read() -> String { "function".to_string() }

fn main() {
    let read = "variable";
    let result = read();
    println!("{}", read);
}"#;
        
        server.did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "rust".to_string(),
                version: 1,
                text: content.to_string(),
            },
        }).await;
        
        // Test 1: Function call should go to function (line 0)
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position { line: 4, character: 17 }, // "read" in read()
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        
        let result = server.goto_definition(params).await.unwrap();
        
        match result {
            Some(GotoDefinitionResponse::Scalar(location)) => {
                assert_eq!(location.range.start.line, 0, 
                    "EXPECTED FAILURE: Function call should jump to function (line 0), not variable (line 3). Got line {}",
                    location.range.start.line);
            }
            _ => panic!("Expected a definition location"),
        }
        
        // Test 2: Variable reference should go to variable (line 3)
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position { line: 5, character: 19 }, // "read" in println!
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        
        let result = server.goto_definition(params).await.unwrap();
        
        match result {
            Some(GotoDefinitionResponse::Scalar(location)) => {
                assert_eq!(location.range.start.line, 3, 
                    "EXPECTED FAILURE: Variable reference should jump to variable (line 3), not function (line 0). Got line {}",
                    location.range.start.line);
            }
            _ => panic!("Expected a definition location"),
        }
    }

    #[tokio::test]
    async fn test_distance_calculation_precision() {
        // RED: This test should FAIL initially - current distance calculation is too simplistic
        let (service, _socket) = LspService::new(TreeSitterLs::new);
        let server = service.inner();
        
        let init_params = InitializeParams {
            capabilities: ClientCapabilities::default(),
            initialization_options: Some(serde_json::json!({
                "languages": {
                    "rust": {
                        "filetypes": ["rs"],
                        "highlight": [{"path": "queries/rust/highlights.scm"}],
                        "locals": [{"path": "queries/rust/locals.scm"}]
                    }
                }
            })),
            ..Default::default()
        };
        
        let _ = server.initialize(init_params).await.unwrap();
        
        let uri = Url::parse("file:///distance_test.rs").unwrap();
        let content = r#"fn main() {
    let x = 1;
    {
        let y = 2;
        {
            let x = 3;
            println!("{}", x);
        }
    }
}"#;
        
        server.did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "rust".to_string(),
                version: 1,
                text: content.to_string(),
            },
        }).await;
        
        // Jump from the reference in innermost println! (line 6)
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position { line: 6, character: 23 }, // "x" in println!
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        
        let result = server.goto_definition(params).await.unwrap();
        
        // Should go to closest definition in scope (line 5), NOT outer scope (line 1)
        match result {
            Some(GotoDefinitionResponse::Scalar(location)) => {
                assert_eq!(location.range.start.line, 5, 
                    "EXPECTED FAILURE: Should jump to closest definition in scope (line 5), not distant one (line 1). Got line {}",
                    location.range.start.line);
            }
            _ => panic!("Expected a definition location"),
        }
    }
}