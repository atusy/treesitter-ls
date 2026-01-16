//! Progress notification helpers for LSP $/progress notifications.
//!
//! This module provides helpers for creating WorkDoneProgress notifications
//! to inform users about background operations like parser auto-installation.

use tower_lsp::lsp_types::{
    NumberOrString, ProgressParams, ProgressParamsValue, WorkDoneProgress, WorkDoneProgressBegin,
    WorkDoneProgressEnd,
};

/// Creates a progress token for a language installation.
///
/// Format: `tree-sitter-ls/install/{language}`
/// Each language gets a unique token to allow concurrent installations.
pub fn progress_token(language: &str) -> NumberOrString {
    NumberOrString::String(format!("tree-sitter-ls/install/{}", language))
}

/// Creates a ProgressParams for the Begin phase of parser installation.
///
/// The title will be "Installing {language} parser..."
pub fn create_progress_begin(language: &str) -> ProgressParams {
    ProgressParams {
        token: progress_token(language),
        value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(WorkDoneProgressBegin {
            title: format!("Installing {} parser...", language),
            cancellable: Some(false),
            message: None,
            percentage: None,
        })),
    }
}

/// Creates a ProgressParams for the End phase of parser installation.
///
/// The message will indicate success or failure.
pub fn create_progress_end(language: &str, success: bool) -> ProgressParams {
    let message = if success {
        format!("{} parser installed successfully", language)
    } else {
        format!("Failed to install {} parser", language)
    };

    ProgressParams {
        token: progress_token(language),
        value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(WorkDoneProgressEnd {
            message: Some(message),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_token_format() {
        let token = progress_token("python");

        // Token should be a string with the format tree-sitter-ls/install/{language}
        match token {
            NumberOrString::String(s) => {
                assert_eq!(s, "tree-sitter-ls/install/python");
            }
            NumberOrString::Number(_) => {
                panic!("Expected String token, got Number");
            }
        }
    }

    #[test]
    fn test_create_progress_begin() {
        let params = create_progress_begin("python");

        // Verify token format
        match &params.token {
            NumberOrString::String(s) => {
                assert_eq!(s, "tree-sitter-ls/install/python");
            }
            NumberOrString::Number(_) => {
                panic!("Expected String token, got Number");
            }
        }

        // Verify it's a Begin variant with correct title
        match params.value {
            ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(begin)) => {
                assert_eq!(begin.title, "Installing python parser...");
                assert_eq!(begin.cancellable, Some(false));
            }
            _ => panic!("Expected WorkDoneProgress::Begin"),
        }
    }

    #[test]
    fn test_create_progress_end() {
        // Test success case
        let params_success = create_progress_end("python", true);

        match &params_success.token {
            NumberOrString::String(s) => {
                assert_eq!(s, "tree-sitter-ls/install/python");
            }
            NumberOrString::Number(_) => panic!("Expected String token"),
        }

        match params_success.value {
            ProgressParamsValue::WorkDone(WorkDoneProgress::End(end)) => {
                assert!(end.message.as_ref().unwrap().contains("python"));
                assert!(
                    end.message
                        .as_ref()
                        .unwrap()
                        .to_lowercase()
                        .contains("success")
                        || end
                            .message
                            .as_ref()
                            .unwrap()
                            .to_lowercase()
                            .contains("installed")
                );
            }
            _ => panic!("Expected WorkDoneProgress::End"),
        }

        // Test failure case
        let params_fail = create_progress_end("lua", false);

        match params_fail.value {
            ProgressParamsValue::WorkDone(WorkDoneProgress::End(end)) => {
                assert!(end.message.as_ref().unwrap().contains("lua"));
                assert!(
                    end.message
                        .as_ref()
                        .unwrap()
                        .to_lowercase()
                        .contains("fail")
                );
            }
            _ => panic!("Expected WorkDoneProgress::End"),
        }
    }
}
