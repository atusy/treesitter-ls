//! Heuristic-based language detection using syntect.
//!
//! This module provides language detection via:
//! - Token matching (e.g., "py", "js", "bash" from code fences)
//! - Shebang lines (e.g., `#!/usr/bin/env python`)
//! - Emacs/Vim mode lines (e.g., `# -*- mode: ruby -*-`)
//!
//! Uses syntect's Sublime Text syntax definitions for comprehensive coverage.
//! Part of the detection fallback chain (ADR-0005).
//!
//! ## Token Extraction from Paths
//!
//! The `extract_token_from_path` function enables unified detection by converting
//! file paths to tokens that can be passed to `detect_from_token`:
//! - Files with extension: `foo.py` → `"py"`
//! - Files without extension: `Makefile` → `"Makefile"`

use std::path::Path;
use std::sync::LazyLock;
use syntect::parsing::SyntaxSet;

/// Lazily initialized syntax set with default syntaxes.
static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

/// Detect language from a token (e.g., "py", "js", "bash").
///
/// Used for code fence language identifiers in Markdown/HTML.
/// Uses syntect's find_syntax_by_token which searches extension list then name.
/// Returns the syntax name in lowercase if found, None otherwise.
pub fn detect_from_token(token: &str) -> Option<String> {
    let syntax = SYNTAX_SET.find_syntax_by_token(token)?;
    Some(normalize_syntax_name(&syntax.name))
}

/// Detect language from file content's first line (shebang, mode line).
///
/// Uses syntect's regex-based detection from Sublime Text syntax definitions.
/// Returns the syntax name in lowercase if found, None otherwise.
pub fn detect_from_first_line(content: &str) -> Option<String> {
    let first_line = content.lines().next()?;
    let syntax = SYNTAX_SET.find_syntax_by_first_line(first_line)?;
    Some(normalize_syntax_name(&syntax.name))
}

/// Extract a token from a file path for language detection.
///
/// This enables unified detection by converting paths to tokens:
/// - Files with extension: `foo.py` → `"py"` (extension)
/// - Files without extension: `Makefile` → `"Makefile"` (basename)
///
/// The returned token can be passed to `detect_from_token` for syntect-based detection.
pub(crate) fn extract_token_from_path(path: &str) -> Option<&str> {
    let path = Path::new(path);
    let filename = path.file_name()?.to_str()?;

    // If file has an extension, use extension; otherwise use basename
    // This handles both "script.py" → "py" and "Makefile" → "Makefile"
    path.extension().and_then(|e| e.to_str()).or(Some(filename))
}

/// Normalize syntect syntax name to Tree-sitter parser name.
///
/// Syntect uses Sublime Text naming (e.g., "JavaScript", "Python")
/// while Tree-sitter uses lowercase (e.g., "javascript", "python").
fn normalize_syntax_name(name: &str) -> String {
    // Common mappings from Sublime Text names to Tree-sitter names
    match name {
        // Shell variants
        "Bourne Again Shell (bash)" => "bash".to_string(),
        "Shell-Unix-Generic" => "bash".to_string(),
        // Common languages with different naming
        "JavaScript" => "javascript".to_string(),
        "TypeScript" => "typescript".to_string(),
        "Python" => "python".to_string(),
        "Ruby" => "ruby".to_string(),
        "Rust" => "rust".to_string(),
        "Go" => "go".to_string(),
        "C++" => "cpp".to_string(),
        "C" => "c".to_string(),
        "Java" => "java".to_string(),
        "Perl" => "perl".to_string(),
        "PHP" => "php".to_string(),
        "Lua" => "lua".to_string(),
        "R" => "r".to_string(),
        "Makefile" => "make".to_string(),
        "Dockerfile" => "dockerfile".to_string(),
        // Default: lowercase the name
        _ => name.to_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Token detection tests (for code fence identifiers)

    #[test]
    fn test_detect_token_py() {
        assert_eq!(detect_from_token("py"), Some("python".to_string()));
    }

    #[test]
    fn test_detect_token_js() {
        assert_eq!(detect_from_token("js"), Some("javascript".to_string()));
    }

    #[test]
    fn test_detect_token_bash() {
        assert_eq!(detect_from_token("bash"), Some("bash".to_string()));
    }

    #[test]
    fn test_detect_token_rust() {
        assert_eq!(detect_from_token("rust"), Some("rust".to_string()));
    }

    #[test]
    fn test_detect_token_unknown() {
        assert_eq!(detect_from_token("unknown_language_xyz"), None);
    }

    // Shebang detection tests

    #[test]
    fn test_detect_shebang_python() {
        let content = "#!/usr/bin/env python\nprint('hello')";
        assert_eq!(detect_from_first_line(content), Some("python".to_string()));
    }

    #[test]
    fn test_detect_shebang_python3() {
        let content = "#!/usr/bin/env python3\nprint('hello')";
        assert_eq!(detect_from_first_line(content), Some("python".to_string()));
    }

    #[test]
    fn test_detect_shebang_bash() {
        let content = "#!/bin/bash\necho hello";
        assert_eq!(detect_from_first_line(content), Some("bash".to_string()));
    }

    #[test]
    fn test_detect_shebang_sh() {
        let content = "#!/bin/sh\necho hello";
        assert_eq!(detect_from_first_line(content), Some("bash".to_string()));
    }

    #[test]
    fn test_detect_shebang_node() {
        let content = "#!/usr/bin/env node\nconsole.log('hello')";
        assert_eq!(
            detect_from_first_line(content),
            Some("javascript".to_string())
        );
    }

    #[test]
    fn test_detect_shebang_ruby() {
        let content = "#!/usr/bin/env ruby\nputs 'hello'";
        assert_eq!(detect_from_first_line(content), Some("ruby".to_string()));
    }

    #[test]
    fn test_detect_shebang_perl() {
        let content = "#!/usr/bin/perl\nprint 'hello';";
        assert_eq!(detect_from_first_line(content), Some("perl".to_string()));
    }

    #[test]
    fn test_detect_no_shebang() {
        assert_eq!(detect_from_first_line("print('hello')"), None);
        assert_eq!(detect_from_first_line(""), None);
    }

    // Token extraction from path tests

    #[test]
    fn test_extract_token_with_extension() {
        // Files with extension return the extension
        assert_eq!(extract_token_from_path("/path/to/file.rs"), Some("rs"));
        assert_eq!(extract_token_from_path("/path/to/script.py"), Some("py"));
        assert_eq!(extract_token_from_path("/path/to/app.js"), Some("js"));
    }

    #[test]
    fn test_extract_token_without_extension() {
        // Files without extension return the basename
        assert_eq!(
            extract_token_from_path("/path/to/Makefile"),
            Some("Makefile")
        );
        assert_eq!(
            extract_token_from_path("/path/to/Dockerfile"),
            Some("Dockerfile")
        );
        assert_eq!(extract_token_from_path("/path/to/Gemfile"), Some("Gemfile"));
    }

    #[test]
    fn test_extract_token_hidden_file() {
        // Hidden files without extension return the basename
        assert_eq!(extract_token_from_path("/home/.bashrc"), Some(".bashrc"));
        assert_eq!(
            extract_token_from_path("/home/.gitignore"),
            Some(".gitignore")
        );
    }

    #[test]
    fn test_extract_token_unknown_file() {
        // Unknown file still extracts basename
        assert_eq!(
            extract_token_from_path("/path/to/random_file"),
            Some("random_file")
        );
    }

    // Combined token extraction + detection tests (integration)

    #[test]
    fn test_path_to_token_to_language_rust() {
        let token = extract_token_from_path("/path/to/main.rs").unwrap();
        assert_eq!(detect_from_token(token), Some("rust".to_string()));
    }

    #[test]
    fn test_path_to_token_to_language_python() {
        let token = extract_token_from_path("/path/to/script.py").unwrap();
        assert_eq!(detect_from_token(token), Some("python".to_string()));
    }

    #[test]
    fn test_path_to_token_to_language_makefile() {
        let token = extract_token_from_path("/path/to/Makefile").unwrap();
        assert_eq!(detect_from_token(token), Some("make".to_string()));
    }

    #[test]
    fn test_path_to_token_to_language_gemfile() {
        let token = extract_token_from_path("/path/to/Gemfile").unwrap();
        // Gemfile is recognized as Ruby
        assert!(detect_from_token(token).is_some());
    }

    #[test]
    fn test_path_to_token_to_language_bashrc() {
        let token = extract_token_from_path("/home/.bashrc").unwrap();
        assert_eq!(detect_from_token(token), Some("bash".to_string()));
    }
}
