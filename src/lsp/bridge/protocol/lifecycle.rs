//! LSP lifecycle message builders.
//!
//! Provides builders for initialize, shutdown, and exit messages
//! used during connection lifecycle management.

use super::request_id::RequestId;

/// Build an LSP initialize request.
///
/// # Arguments
/// * `request_id` - The JSON-RPC request ID
/// * `initialization_options` - Server-specific initialization options
pub(crate) fn build_initialize_request(
    request_id: RequestId,
    initialization_options: Option<serde_json::Value>,
) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id.as_i64(),
        "method": "initialize",
        "params": {
            "processId": std::process::id(),
            "rootUri": null,
            "capabilities": {},
            "initializationOptions": initialization_options
        }
    })
}

/// Build an LSP initialized notification.
///
/// Sent after receiving the initialize response to signal
/// that the client is ready to receive requests.
pub(crate) fn build_initialized_notification() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {}
    })
}

/// Build an LSP shutdown request.
///
/// # Arguments
/// * `request_id` - The JSON-RPC request ID
pub(crate) fn build_shutdown_request(request_id: RequestId) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id.as_i64(),
        "method": "shutdown",
        "params": null
    })
}

/// Build an LSP exit notification.
///
/// Sent after receiving the shutdown response to terminate the server.
pub(crate) fn build_exit_notification() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "exit",
        "params": null
    })
}

/// Build a textDocument/didClose notification.
///
/// # Arguments
/// * `uri` - The URI of the document being closed
pub(crate) fn build_didclose_notification(uri: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didClose",
        "params": {
            "textDocument": {
                "uri": uri
            }
        }
    })
}

/// Validates a JSON-RPC initialize response.
///
/// Uses lenient interpretation to maximize compatibility with non-conformant servers:
/// - Prioritizes error field if present and non-null
/// - Accepts result with null error field (`{"result": {...}, "error": null}`)
/// - Rejects null or missing result field
///
/// # Returns
/// * `Ok(())` - Response is valid (has non-null result, no error)
/// * `Err(e)` - Response has error or missing/null result
pub(crate) fn validate_initialize_response(response: &serde_json::Value) -> std::io::Result<()> {
    // 1. Check for error response (prioritize error if present)
    if let Some(error) = response.get("error").filter(|e| !e.is_null()) {
        // Error field is non-null: treat as error regardless of result
        let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
        let message = error
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");

        return Err(std::io::Error::other(format!(
            "bridge: initialize failed (code {}): {}",
            code, message
        )));
    }

    // 2. Reject if result is absent or null
    if response.get("result").filter(|r| !r.is_null()).is_none() {
        return Err(std::io::Error::other(
            "bridge: initialize response missing valid result",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn initialize_request_has_correct_structure() {
        let request = build_initialize_request(RequestId::new(1), None);

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 1);
        assert_eq!(request["method"], "initialize");
        assert!(request["params"]["processId"].as_u64().is_some());
        assert!(request["params"]["rootUri"].is_null());
        assert!(request["params"]["capabilities"].is_object());
        assert!(request["params"]["initializationOptions"].is_null());
    }

    #[test]
    fn initialize_request_includes_initialization_options() {
        let options = serde_json::json!({
            "settings": {
                "lua": {
                    "diagnostics": { "globals": ["vim"] }
                }
            }
        });
        let request = build_initialize_request(RequestId::new(42), Some(options.clone()));

        assert_eq!(request["id"], 42);
        assert_eq!(request["params"]["initializationOptions"], options);
    }

    #[test]
    fn initialized_notification_has_correct_structure() {
        let notification = build_initialized_notification();

        assert_eq!(notification["jsonrpc"], "2.0");
        assert_eq!(notification["method"], "initialized");
        assert!(notification["params"].is_object());
        assert!(notification.get("id").is_none());
    }

    #[test]
    fn shutdown_request_has_correct_structure() {
        let request = build_shutdown_request(RequestId::new(99));

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 99);
        assert_eq!(request["method"], "shutdown");
        assert!(request["params"].is_null());
    }

    #[test]
    fn exit_notification_has_correct_structure() {
        let notification = build_exit_notification();

        assert_eq!(notification["jsonrpc"], "2.0");
        assert_eq!(notification["method"], "exit");
        assert!(notification["params"].is_null());
        assert!(notification.get("id").is_none());
    }

    #[test]
    fn didclose_notification_has_correct_structure() {
        let notification = build_didclose_notification("file:///project/test.lua");

        assert_eq!(notification["jsonrpc"], "2.0");
        assert_eq!(notification["method"], "textDocument/didClose");
        assert_eq!(
            notification["params"]["textDocument"]["uri"],
            "file:///project/test.lua"
        );
        assert!(notification.get("id").is_none());
    }

    #[test]
    fn didclose_notification_with_virtual_uri() {
        let uri = "file:///.kakehashi/lua/abc123.lua";
        let notification = build_didclose_notification(uri);

        assert_eq!(notification["params"]["textDocument"]["uri"], uri);
    }

    // Tests for validate_initialize_response

    #[rstest]
    #[case::valid_result_without_error(
        serde_json::json!({"result": {"capabilities": {}}})
    )]
    #[case::valid_result_with_null_error(
        serde_json::json!({"result": {"capabilities": {}}, "error": null})
    )]
    #[case::complex_result_object(
        serde_json::json!({
            "result": {
                "capabilities": {
                    "textDocumentSync": 1,
                    "completionProvider": {
                        "triggerCharacters": ["."]
                    }
                },
                "serverInfo": {
                    "name": "test-server",
                    "version": "1.0.0"
                }
            }
        })
    )]
    fn validate_accepts_valid_response(#[case] response: serde_json::Value) {
        assert!(validate_initialize_response(&response).is_ok());
    }

    #[rstest]
    #[case::null_result(
        serde_json::json!({"result": null}),
        "missing valid result"
    )]
    #[case::missing_result_and_error(
        serde_json::json!({}),
        "missing valid result"
    )]
    #[case::null_result_with_null_error(
        serde_json::json!({"result": null, "error": null}),
        "missing valid result"
    )]
    fn validate_rejects_missing_result(
        #[case] response: serde_json::Value,
        #[case] expected_error: &str,
    ) {
        let result = validate_initialize_response(&response);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(expected_error));
    }

    #[rstest]
    #[case::error_response(
        serde_json::json!({
            "error": {
                "code": -32600,
                "message": "Invalid Request"
            }
        }),
        "code -32600",
        "Invalid Request"
    )]
    #[case::error_response_even_with_result(
        serde_json::json!({
            "result": {"capabilities": {}},
            "error": {
                "code": -32603,
                "message": "Internal error"
            }
        }),
        "code -32603",
        "Internal error"
    )]
    #[case::malformed_error_missing_code(
        serde_json::json!({
            "error": {
                "message": "Something went wrong"
            }
        }),
        "code -1",  // Default code
        "Something went wrong"
    )]
    #[case::malformed_error_missing_message(
        serde_json::json!({
            "error": {
                "code": -32700
            }
        }),
        "code -32700",
        "unknown error"  // Default message
    )]
    #[case::malformed_error_empty_object(
        serde_json::json!({"error": {}}),
        "code -1",
        "unknown error"
    )]
    #[case::malformed_error_wrong_types(
        serde_json::json!({
            "error": {
                "code": "not-a-number",
                "message": 123
            }
        }),
        "code -1",  // Can't parse string as i64
        "unknown error"  // Can't parse number as str
    )]
    fn validate_rejects_error_response(
        #[case] response: serde_json::Value,
        #[case] expected_code: &str,
        #[case] expected_message: &str,
    ) {
        let result = validate_initialize_response(&response);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains(expected_code));
        assert!(err_msg.contains(expected_message));
    }
}
