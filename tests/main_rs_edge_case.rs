// Test for the specific edge case in src/bin/main.rs
use serde_json;
use tower_lsp::lsp_types::*;
use tower_lsp::{LanguageServer, LspService};
use treesitter_ls::*;

#[tokio::test]
async fn test_main_rs_stdin_edge_case() {
    // This test specifically addresses the edge case you identified:
    // In `let stdin = stdin();`, the `stdin()` call should go to the import,
    // not the variable being declared on the left side.

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

    let uri = Url::parse("file:///main.rs").unwrap();

    // Exact content from src/bin/main.rs
    let content = r#"use tokio::io::{stdin, stdout};
use tower_lsp::{LspService, Server};
use treesitter_ls::TreeSitterLs;

#[tokio::main]
async fn main() {
    let stdin = stdin();
    let stdout = stdout();

    let (service, socket) = LspService::new(TreeSitterLs::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}"#;

    server
        .did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "rust".to_string(),
                version: 1,
                text: content.to_string(),
            },
        })
        .await;

    // Test 1: stdin() function call on line 6 should go to IMPORT (line 0), not variable
    let params = GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: Position {
                line: 6,
                character: 16,
            }, // "stdin" in stdin()
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = server.goto_definition(params).await.unwrap();

    match result {
        Some(GotoDefinitionResponse::Scalar(location)) => {
            assert_eq!(
                location.range.start.line, 0,
                "stdin() function call should jump to IMPORT (line 0), not local variable (line 6). Got line {}",
                location.range.start.line
            );

            // Should point to the import in the use statement
            assert!(
                location.range.start.character >= 17 && location.range.start.character <= 22,
                "Should point to 'stdin' in the import, got column {}",
                location.range.start.character
            );
        }
        _ => panic!("Expected a definition location for stdin() function call"),
    }

    // Test 2: stdin variable reference on line 10 should go to VARIABLE (line 6), not import
    let params = GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: Position {
                line: 10,
                character: 16,
            }, // "stdin" in Server::new(stdin, ...)
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };

    let result = server.goto_definition(params).await.unwrap();

    match result {
        Some(GotoDefinitionResponse::Scalar(location)) => {
            assert_eq!(
                location.range.start.line, 6,
                "stdin variable reference should jump to LOCAL VARIABLE (line 6), not import (line 0). Got line {}",
                location.range.start.line
            );

            // Should point to the variable declaration
            assert!(
                location.range.start.character >= 8 && location.range.start.character <= 13,
                "Should point to 'stdin' variable declaration, got column {}",
                location.range.start.character
            );
        }
        _ => panic!("Expected a definition location for stdin variable reference"),
    }

    println!("✅ main.rs edge case handled correctly!");
    println!("   - stdin() call → import");
    println!("   - stdin variable → local declaration");
}

#[test]
fn test_context_matching_includes_imports() {
    // Test that imports are considered valid targets for function calls
    use treesitter_ls::{ContextType, DefinitionResolver};

    let resolver = DefinitionResolver::new();

    // Function calls should match imports (for imported functions)
    assert!(
        resolver.context_matches("import", &ContextType::FunctionCall),
        "Imports should be valid targets for function calls"
    );

    // But variable references should not prefer imports over variables
    assert!(
        !resolver.context_matches("import", &ContextType::VariableReference),
        "Variable references should not prefer imports"
    );

    println!("✅ Context matching correctly handles imports for function calls!");
}
