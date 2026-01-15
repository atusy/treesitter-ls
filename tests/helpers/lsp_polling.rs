//! LSP polling utilities for E2E tests.
//!
//! Provides retry-with-timeout patterns for waiting on async LSP responses.

use std::time::Duration;

use super::lsp_client::LspClient;
use serde_json::json;

/// Poll until a predicate returns Some(T) or max attempts reached.
///
/// # Arguments
/// * `max_attempts` - Maximum number of polling attempts
/// * `delay_ms` - Delay between attempts in milliseconds
/// * `predicate` - Function that returns Some(T) on success, None to retry
///
/// # Returns
/// * `Some(T)` if predicate succeeded within max_attempts
/// * `None` if max_attempts reached without success
pub fn poll_until<T, F>(max_attempts: usize, delay_ms: u64, mut predicate: F) -> Option<T>
where
    F: FnMut() -> Option<T>,
{
    for attempt in 1..=max_attempts {
        if let Some(result) = predicate() {
            eprintln!(
                "poll_until succeeded on attempt {}/{}",
                attempt, max_attempts
            );
            return Some(result);
        }

        if attempt < max_attempts {
            std::thread::sleep(Duration::from_millis(delay_ms));
        }
    }

    eprintln!("poll_until exhausted {} attempts", max_attempts);
    None
}

/// Poll for LSP position-based request results with retries.
///
/// Many LSP requests (hover, completion, definition, etc.) need time for the downstream
/// language server to index files. This helper retries until a non-null result is returned.
///
/// # Arguments
/// * `client` - The LSP client to send requests through
/// * `method` - The LSP method (e.g., "textDocument/hover", "textDocument/completion")
/// * `uri` - The document URI
/// * `line` - Zero-indexed line number
/// * `character` - Zero-indexed character position
/// * `max_attempts` - Maximum number of polling attempts
/// * `delay_ms` - Delay between attempts in milliseconds
///
/// # Returns
/// * `Some(response)` - The full JSON-RPC response if a non-null result was received
/// * `None` - If max_attempts reached without a successful non-null response
pub fn poll_for_lsp_result(
    client: &mut LspClient,
    method: &str,
    uri: &str,
    line: u32,
    character: u32,
    max_attempts: u32,
    delay_ms: u64,
) -> Option<serde_json::Value> {
    for attempt in 1..=max_attempts {
        let response = client.send_request(
            method,
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        );

        if response.get("error").is_some() {
            eprintln!(
                "{} attempt {}/{}: Error: {:?}",
                method,
                attempt,
                max_attempts,
                response.get("error")
            );
            std::thread::sleep(Duration::from_millis(delay_ms));
            continue;
        }

        if let Some(result) = response.get("result") {
            if !result.is_null() {
                eprintln!(
                    "{} succeeded on attempt {}/{}",
                    method, attempt, max_attempts
                );
                return Some(response);
            }
        }

        eprintln!(
            "{} attempt {}/{}: null result, retrying...",
            method, attempt, max_attempts
        );
        std::thread::sleep(Duration::from_millis(delay_ms));
    }

    eprintln!(
        "{} exhausted {} attempts without non-null result",
        method, max_attempts
    );
    None
}

/// Poll for hover results with retries.
///
/// Convenience wrapper around [`poll_for_lsp_result`] for hover requests.
pub fn poll_for_hover(
    client: &mut LspClient,
    uri: &str,
    line: u32,
    character: u32,
    max_attempts: u32,
    delay_ms: u64,
) -> Option<serde_json::Value> {
    poll_for_lsp_result(
        client,
        "textDocument/hover",
        uri,
        line,
        character,
        max_attempts,
        delay_ms,
    )
}

/// Poll for completion results with retries.
///
/// Convenience wrapper around [`poll_for_lsp_result`] for completion requests.
pub fn poll_for_completions(
    client: &mut LspClient,
    uri: &str,
    line: u32,
    character: u32,
    max_attempts: u32,
    delay_ms: u64,
) -> Option<serde_json::Value> {
    poll_for_lsp_result(
        client,
        "textDocument/completion",
        uri,
        line,
        character,
        max_attempts,
        delay_ms,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_poll_until_succeeds_immediately() {
        let result = poll_until(5, 10, || Some(42));
        assert_eq!(result, Some(42));
    }

    #[test]
    fn test_poll_until_succeeds_after_retries() {
        let mut counter = 0;
        let result = poll_until(5, 10, || {
            counter += 1;
            if counter >= 3 { Some(counter) } else { None }
        });
        assert_eq!(result, Some(3));
    }

    #[test]
    fn test_poll_until_exhausts_attempts() {
        let result = poll_until(3, 10, || None::<i32>);
        assert_eq!(result, None);
    }
}
