//! Language alias normalization for the fallback chain.
//!
//! Per ADR-0005, this module normalizes common language aliases to their
//! canonical parser names. The detection chain first tries the identifier
//! directly, then normalizes if that fails.

/// Normalize common language aliases to canonical parser names.
///
/// This is used in the injection fallback chain (ADR-0005) when
/// the direct identifier doesn't have an available parser.
///
/// # Examples
/// - `py` -> `python`
/// - `js` -> `javascript`
/// - `sh` -> `bash`
///
/// Non-alias identifiers return `None` to indicate no normalization needed.
pub fn normalize_alias(identifier: &str) -> Option<String> {
    match identifier {
        "py" => Some("python".to_string()),
        "js" => Some("javascript".to_string()),
        "sh" => Some("bash".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_alias_py() {
        assert_eq!(normalize_alias("py"), Some("python".to_string()));
    }

    #[test]
    fn test_normalize_alias_js() {
        assert_eq!(normalize_alias("js"), Some("javascript".to_string()));
    }

    #[test]
    fn test_normalize_alias_sh() {
        assert_eq!(normalize_alias("sh"), Some("bash".to_string()));
    }

    #[test]
    fn test_normalize_alias_passthrough() {
        // Non-aliases return None (pass through unchanged)
        assert_eq!(normalize_alias("python"), None);
        assert_eq!(normalize_alias("javascript"), None);
        assert_eq!(normalize_alias("bash"), None);
        assert_eq!(normalize_alias("rust"), None);
        assert_eq!(normalize_alias("unknown"), None);
    }
}
