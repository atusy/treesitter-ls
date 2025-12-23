//! Extension-based language detection for the fallback chain.
//!
//! This module extracts file extensions as parser name candidates.
//! Per ADR-0005, it simply strips the dot: `.rs` → `rs`.
//! No mapping is done - the extension IS the candidate parser name.

/// Extract extension from path as parser name candidate.
///
/// Returns the extension without the leading dot, or None if no extension.
/// This is the final step in the detection fallback chain (ADR-0005).
pub fn detect_from_extension(path: &str) -> Option<String> {
    // Find the last component of the path
    let filename = path.rsplit('/').next().unwrap_or(path);

    // Find the extension (after the last dot, if any)
    if let Some(dot_pos) = filename.rfind('.') {
        // Make sure it's not a hidden file with no extension (e.g., ".bashrc")
        if dot_pos > 0 {
            let ext = &filename[dot_pos + 1..];
            if !ext.is_empty() {
                return Some(ext.to_string());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_extension_rs() {
        assert_eq!(
            detect_from_extension("/path/to/file.rs"),
            Some("rs".to_string())
        );
    }

    #[test]
    fn test_detect_extension_py() {
        assert_eq!(
            detect_from_extension("/path/to/script.py"),
            Some("py".to_string())
        );
    }

    #[test]
    fn test_detect_extension_none() {
        // No extension
        assert_eq!(detect_from_extension("/path/to/Makefile"), None);
        // Hidden file with no extension
        assert_eq!(detect_from_extension("/home/.bashrc"), None);
    }

    #[test]
    fn test_detect_extension_strips_dot() {
        // Verify no leading dot in result
        let result = detect_from_extension("/path/to/file.rs");
        assert_eq!(result, Some("rs".to_string()));
        assert!(!result.unwrap().starts_with('.'));
    }
}
