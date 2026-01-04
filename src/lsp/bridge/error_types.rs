//! LSP-compliant error types for bridge communication.
//!
//! This module provides error codes and structures that comply with the LSP 3.x
//! Response Message specification, ensuring all errors are properly structured
//! and use standard error codes.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// LSP-compliant error codes (LSP 3.17+)
pub struct ErrorCodes;

impl ErrorCodes {
    /// Request failed but was syntactically correct (LSP 3.17)
    /// Use for: downstream server failures, timeouts, circuit breaker open
    pub const REQUEST_FAILED: i32 = -32803;

    /// Server cancelled the request (LSP 3.17)
    /// Only for requests that explicitly support server cancellation
    pub const SERVER_CANCELLED: i32 = -32802;

    /// Server not initialized (JSON-RPC reserved)
    /// Use for: requests/notifications sent before `initialized`
    pub const SERVER_NOT_INITIALIZED: i32 = -32002;
}

/// LSP-compliant error response structure (LSP 3.x § Response Message)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseError {
    /// LSP error code
    pub code: i32,
    /// Human-readable error message
    pub message: String,
    /// Optional additional error data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl ResponseError {
    /// Create a REQUEST_FAILED error for timeout scenarios
    pub fn timeout(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCodes::REQUEST_FAILED,
            message: message.into(),
            data: Some(serde_json::json!({"reason": "timeout"})),
        }
    }

    /// Create a SERVER_NOT_INITIALIZED error
    pub fn not_initialized(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCodes::SERVER_NOT_INITIALIZED,
            message: message.into(),
            data: None,
        }
    }

    /// Create a REQUEST_FAILED error for general failures
    pub fn request_failed(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCodes::REQUEST_FAILED,
            message: message.into(),
            data: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_response_error_timeout_helper() {
        let error = ResponseError::timeout("Initialization timeout after 5s");

        assert_eq!(error.code, ErrorCodes::REQUEST_FAILED);
        assert_eq!(error.message, "Initialization timeout after 5s");
        assert_eq!(error.data.as_ref().unwrap()["reason"], "timeout");
    }

    #[test]
    fn test_response_error_not_initialized_helper() {
        let error = ResponseError::not_initialized("Server not ready");

        assert_eq!(error.code, ErrorCodes::SERVER_NOT_INITIALIZED);
        assert_eq!(error.message, "Server not ready");
        assert!(error.data.is_none());
    }

    #[test]
    fn test_response_error_request_failed_helper() {
        let error = ResponseError::request_failed("Downstream server crashed");

        assert_eq!(error.code, ErrorCodes::REQUEST_FAILED);
        assert_eq!(error.message, "Downstream server crashed");
        assert!(error.data.is_none());
    }

    #[test]
    fn test_response_error_serialization() {
        // Test basic error with code and message
        let error = ResponseError {
            code: ErrorCodes::REQUEST_FAILED,
            message: "Request failed".to_string(),
            data: None,
        };

        let serialized = serde_json::to_value(&error).unwrap();
        assert_eq!(serialized["code"], -32803);
        assert_eq!(serialized["message"], "Request failed");
        assert!(serialized.get("data").is_none() || serialized["data"].is_null());
    }

    #[test]
    fn test_response_error_with_data() {
        // Test error with additional data field
        let error = ResponseError {
            code: ErrorCodes::SERVER_NOT_INITIALIZED,
            message: "Server not ready".to_string(),
            data: Some(json!({"reason": "initialization_timeout"})),
        };

        let serialized = serde_json::to_value(&error).unwrap();
        assert_eq!(serialized["code"], -32002);
        assert_eq!(serialized["message"], "Server not ready");
        assert_eq!(serialized["data"]["reason"], "initialization_timeout");
    }

    #[test]
    fn test_error_codes_constants() {
        // Verify LSP-compliant error codes
        assert_eq!(ErrorCodes::REQUEST_FAILED, -32803);
        assert_eq!(ErrorCodes::SERVER_NOT_INITIALIZED, -32002);
        assert_eq!(ErrorCodes::SERVER_CANCELLED, -32802);
    }

    #[test]
    fn test_lsp_json_rpc_error_response_structure() {
        // Test that ResponseError can be embedded in a full LSP error response
        let error = ResponseError {
            code: ErrorCodes::REQUEST_FAILED,
            message: "Downstream server timeout".to_string(),
            data: Some(json!({"server": "pyright", "timeout_ms": 5000})),
        };

        let full_response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "error": error
        });

        assert_eq!(full_response["jsonrpc"], "2.0");
        assert_eq!(full_response["id"], 42);
        assert_eq!(full_response["error"]["code"], -32803);
        assert_eq!(
            full_response["error"]["message"],
            "Downstream server timeout"
        );
        assert_eq!(full_response["error"]["data"]["server"], "pyright");
    }

    #[test]
    fn test_response_error_skip_none_data() {
        // Test that None data field is not serialized (skip_serializing_if)
        let error = ResponseError {
            code: ErrorCodes::SERVER_CANCELLED,
            message: "Request cancelled".to_string(),
            data: None,
        };

        let serialized = serde_json::to_value(&error).unwrap();
        // The data field should not be present when None
        assert!(!serialized.as_object().unwrap().contains_key("data"));
    }
}
