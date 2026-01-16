//! Error handling types for tree-sitter-ls
//!
//! This module provides error types used throughout the LSP server.

use std::sync::PoisonError;
use thiserror::Error;

/// Comprehensive error type for LSP operations
#[derive(Debug, Error)]
pub enum LspError {
    /// Lock acquisition failed or was poisoned
    #[error("Lock acquisition failed: {message}")]
    Lock { message: String },

    /// Parser not found for the specified language
    #[error("Parser not found for language: {language}")]
    ParserNotFound { language: String },

    /// Language configuration not found
    #[error("Language not found: {language}")]
    LanguageNotFound { language: String },

    /// Configuration error
    #[error("Invalid configuration: {message}")]
    Config { message: String },

    /// Query execution or parsing failed
    #[error("Query error: {message}")]
    Query { message: String },

    /// Document not found in store
    #[error("Document not found: {uri}")]
    DocumentNotFound { uri: String },

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type for LSP operations
pub type LspResult<T> = Result<T, LspError>;

/// Helper trait to convert PoisonError to LspError
pub trait LockResultExt<T> {
    /// Convert a PoisonError to LspError with recovery and logging.
    ///
    /// The context parameter identifies which operation triggered lock recovery,
    /// helping developers debug thread safety issues.
    fn recover_poison(self, context: &str) -> Result<T, LspError>;
}

impl<T> LockResultExt<T> for Result<T, PoisonError<T>> {
    fn recover_poison(self, context: &str) -> Result<T, LspError> {
        match self {
            Ok(guard) => Ok(guard),
            Err(poisoned) => {
                log::warn!(
                    target: "tree_sitter_ls::lock_recovery",
                    "Recovered from poisoned lock in {}",
                    context
                );
                Ok(poisoned.into_inner())
            }
        }
    }
}

/// Helper functions for common error patterns
impl LspError {
    /// Create a lock error
    pub fn lock(message: impl Into<String>) -> Self {
        LspError::Lock {
            message: message.into(),
        }
    }

    /// Create a parser not found error
    pub fn parser_not_found(language: impl Into<String>) -> Self {
        LspError::ParserNotFound {
            language: language.into(),
        }
    }

    /// Create a language not found error
    pub fn language_not_found(language: impl Into<String>) -> Self {
        LspError::LanguageNotFound {
            language: language.into(),
        }
    }

    /// Create a configuration error
    pub fn config(message: impl Into<String>) -> Self {
        LspError::Config {
            message: message.into(),
        }
    }

    /// Create a query error
    pub fn query(message: impl Into<String>) -> Self {
        LspError::Query {
            message: message.into(),
        }
    }

    /// Create a document not found error
    pub fn document_not_found(uri: impl Into<String>) -> Self {
        LspError::DocumentNotFound { uri: uri.into() }
    }

    /// Create an internal error
    pub fn internal(message: impl Into<String>) -> Self {
        LspError::Internal(message.into())
    }
}
