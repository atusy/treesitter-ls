//! Progress notification helpers for LSP $/progress notifications.
//!
//! This module provides helpers for creating WorkDoneProgress notifications
//! to inform users about background operations like parser auto-installation.

use tower_lsp::lsp_types::NumberOrString;

/// Creates a progress token for a language installation.
///
/// Format: `treesitter-ls/install/{language}`
/// Each language gets a unique token to allow concurrent installations.
pub fn progress_token(language: &str) -> NumberOrString {
    NumberOrString::String(format!("treesitter-ls/install/{}", language))
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
}
