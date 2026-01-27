//! Heuristic-based language detection using syntect.
//!
//! This module provides language detection via:
//! - Token matching (e.g., "py", "js", "bash" from code fences)
//! - Shebang lines (e.g., `#!/usr/bin/env python`)
//! - Emacs/Vim mode lines (e.g., `# -*- mode: ruby -*-`)
//! - File name patterns (e.g., `Makefile`, `Dockerfile`)
//!
//! Uses syntect's Sublime Text syntax definitions for comprehensive coverage.
//! Part of the detection fallback chain (ADR-0005).

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

/// Detect language from file name pattern (e.g., `Makefile`, `Dockerfile`).
///
/// Tries two strategies:
/// 1. Full filename as token (for special files like Makefile, Gemfile, Dockerfile)
/// 2. File extension only (for regular files like file.rs, script.py)
///
/// Returns the syntax name in lowercase if found, None otherwise.
pub fn detect_from_filename(path: &str) -> Option<String> {
    let path = Path::new(path);
    let filename = path.file_name()?.to_str()?;

    // 1. Try full filename (handles Makefile, Gemfile, Dockerfile, etc.)
    //    find_syntax_by_token searches: extension list first, then syntax name
    if let Some(syntax) = SYNTAX_SET.find_syntax_by_token(filename) {
        return Some(normalize_syntax_name(&syntax.name));
    }

    // 2. Try file extension only (handles file.rs, script.py, etc.)
    let extension = path.extension()?.to_str()?;
    let syntax = SYNTAX_SET.find_syntax_by_extension(extension)?;
    Some(normalize_syntax_name(&syntax.name))
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

    // File name pattern tests

    #[test]
    fn test_detect_makefile() {
        assert_eq!(
            detect_from_filename("/path/to/Makefile"),
            Some("make".to_string())
        );
    }

    #[test]
    fn test_detect_dockerfile() {
        // Note: syntect may not have Dockerfile in default syntaxes
        // This test verifies the function doesn't panic on Dockerfile
        let result = detect_from_filename("/path/to/Dockerfile");
        // If syntect supports it, we get Some; otherwise None is acceptable
        assert!(result.is_none() || result == Some("dockerfile".to_string()));
    }

    #[test]
    fn test_detect_gemfile() {
        // Gemfile is recognized as Ruby
        let result = detect_from_filename("/path/to/Gemfile");
        assert!(result.is_some());
    }

    #[test]
    fn test_detect_unknown_filename() {
        assert_eq!(detect_from_filename("/path/to/random_file"), None);
    }

    // Extension-based detection tests

    #[test]
    fn test_detect_rust_by_extension() {
        assert_eq!(
            detect_from_filename("/path/to/main.rs"),
            Some("rust".to_string())
        );
    }

    #[test]
    fn test_detect_python_by_extension() {
        assert_eq!(
            detect_from_filename("/path/to/script.py"),
            Some("python".to_string())
        );
    }

    #[test]
    fn test_detect_javascript_by_extension() {
        assert_eq!(
            detect_from_filename("/path/to/app.js"),
            Some("javascript".to_string())
        );
    }
}
