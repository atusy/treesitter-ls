//! Shebang-based language detection for extensionless scripts.
//!
//! This module detects languages from shebang lines (e.g., `#!/usr/bin/env python`).
//! Used by the detection fallback chain (ADR-0005) when languageId is unavailable.

/// Detect language from shebang line in file content.
///
/// Returns the language name if a valid shebang is found, None otherwise.
/// Only reads the first line of content (lazy I/O friendly).
pub fn detect_from_shebang(content: &str) -> Option<String> {
    let first_line = content.lines().next()?;

    if !first_line.starts_with("#!") {
        return None;
    }

    // Extract the interpreter from shebang
    // Handles both "/usr/bin/env python" and "/bin/bash" styles
    let shebang = first_line.trim_start_matches("#!");
    let interpreter = shebang.split_whitespace().last()?; // Get the last part (actual interpreter)

    // Map interpreter to language name
    interpreter_to_language(interpreter)
}

/// Map interpreter name to Tree-sitter language name
fn interpreter_to_language(interpreter: &str) -> Option<String> {
    // Extract just the binary name from path (e.g., "/bin/bash" -> "bash")
    let binary = interpreter.rsplit('/').next().unwrap_or(interpreter);

    match binary {
        "python" | "python3" | "python2" => Some("python".to_string()),
        "bash" | "sh" | "zsh" => Some("bash".to_string()),
        "node" | "nodejs" => Some("javascript".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_shebang_python() {
        let content = "#!/usr/bin/env python\nprint('hello')";
        assert_eq!(detect_from_shebang(content), Some("python".to_string()));
    }

    #[test]
    fn test_detect_shebang_bash() {
        let content = "#!/bin/bash\necho hello";
        assert_eq!(detect_from_shebang(content), Some("bash".to_string()));
    }

    #[test]
    fn test_detect_shebang_node() {
        let content = "#!/usr/bin/env node\nconsole.log('hello')";
        assert_eq!(detect_from_shebang(content), Some("javascript".to_string()));
    }

    #[test]
    fn test_detect_shebang_none() {
        // No shebang
        assert_eq!(detect_from_shebang("print('hello')"), None);
        // Empty content
        assert_eq!(detect_from_shebang(""), None);
        // Unknown interpreter
        assert_eq!(detect_from_shebang("#!/usr/bin/env unknown_lang"), None);
    }
}
