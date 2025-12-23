//! Progress notification helpers for LSP $/progress notifications.
//!
//! This module provides helpers for creating WorkDoneProgress notifications
//! to inform users about background operations like parser auto-installation.

use tower_lsp::lsp_types::{
    NumberOrString, ProgressParams, ProgressParamsValue, WorkDoneProgress, WorkDoneProgressBegin,
};

/// Creates a progress token for a language installation.
///
/// Format: `treesitter-ls/install/{language}`
/// Each language gets a unique token to allow concurrent installations.
pub fn progress_token(language: &str) -> NumberOrString {
    NumberOrString::String(format!("treesitter-ls/install/{}", language))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_token_format() {
        let token = progress_token("python");

        // Token should be a string with the format treesitter-ls/install/{language}
        match token {
            NumberOrString::String(s) => {
                assert_eq!(s, "treesitter-ls/install/python");
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
                assert_eq!(s, "treesitter-ls/install/python");
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
}
