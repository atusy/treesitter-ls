//! Registry for tracking parsers that have crashed.
//!
//! This module provides crash resilience by:
//! 1. Tracking which parser is currently being used (parsing-in-progress state)
//! 2. Marking parsers as failed when crashes are detected
//! 3. Preventing failed parsers from being loaded again
//!
//! The design handles C assertion failures (SIGABRT) that cannot be caught:
//! - Before parsing, we record the parser being used to a state file
//! - If the process crashes, on restart we detect the crash and mark that parser as failed
//! - Failed parsers are skipped, allowing other languages to continue working

use dashmap::DashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Registry for tracking failed parsers.
///
/// Thread-safe registry that persists failed parser state to disk
/// to survive process restarts.
#[derive(Clone)]
pub struct FailedParserRegistry {
    /// In-memory set of failed parsers for fast lookup
    failed: Arc<DashSet<String>>,
    /// Directory where state files are stored
    state_dir: PathBuf,
}

impl FailedParserRegistry {
    /// Create a new registry with the given state directory.
    pub fn new(state_dir: &Path) -> Self {
        Self {
            failed: Arc::new(DashSet::new()),
            state_dir: state_dir.to_path_buf(),
        }
    }

    /// Path to the "parsing in progress" state file.
    fn parsing_state_path(&self) -> PathBuf {
        self.state_dir.join("parsing_in_progress")
    }

    /// Path to the "failed parsers" list file.
    fn failed_parsers_path(&self) -> PathBuf {
        self.state_dir.join("failed_parsers")
    }

    /// Initialize the registry by checking for crash recovery.
    ///
    /// This should be called on server startup. If a previous parsing
    /// operation was in progress (crash detected), mark that parser as failed.
    pub fn init(&self) -> io::Result<()> {
        // Ensure state directory exists
        fs::create_dir_all(&self.state_dir)?;

        // Load previously failed parsers
        self.load_failed_parsers()?;

        // Check for crash recovery
        let parsing_state = self.parsing_state_path();
        if parsing_state.exists() {
            // Previous parsing was interrupted - crash detected!
            if let Ok(language) = fs::read_to_string(&parsing_state) {
                let language = language.trim();
                if !language.is_empty() {
                    log::error!(
                        target: "treesitter_ls::crash_recovery",
                        "Detected crash during parsing of '{}'. Marking as failed.",
                        language
                    );
                    self.mark_failed(language)?;
                }
            }
            // Clean up state file
            let _ = fs::remove_file(&parsing_state);
        }

        Ok(())
    }

    /// Load the list of failed parsers from disk.
    fn load_failed_parsers(&self) -> io::Result<()> {
        let path = self.failed_parsers_path();
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            for line in content.lines() {
                let lang = line.trim();
                if !lang.is_empty() {
                    self.failed.insert(lang.to_string());
                }
            }
        }
        Ok(())
    }

    /// Save the list of failed parsers to disk.
    fn save_failed_parsers(&self) -> io::Result<()> {
        let path = self.failed_parsers_path();
        let languages: Vec<String> = self.failed.iter().map(|r| r.clone()).collect();
        fs::write(&path, languages.join("\n"))
    }

    /// Check if a parser has failed previously.
    pub fn is_failed(&self, language: &str) -> bool {
        self.failed.contains(language)
    }

    /// Mark a parser as failed.
    pub fn mark_failed(&self, language: &str) -> io::Result<()> {
        self.failed.insert(language.to_string());
        self.save_failed_parsers()
    }

    /// Clear a failed parser (e.g., after reinstallation).
    pub fn clear_failed(&self, language: &str) -> io::Result<()> {
        self.failed.remove(language);
        self.save_failed_parsers()
    }

    /// Record that parsing is starting for a language.
    ///
    /// This writes a state file that will be checked on next startup
    /// to detect if parsing crashed.
    pub fn begin_parsing(&self, language: &str) -> io::Result<()> {
        fs::create_dir_all(&self.state_dir)?;
        fs::write(self.parsing_state_path(), language)
    }

    /// Record that parsing completed successfully.
    ///
    /// This removes the state file, indicating no crash occurred.
    pub fn end_parsing(&self) -> io::Result<()> {
        let path = self.parsing_state_path();
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Get list of all failed parsers.
    pub fn failed_parsers(&self) -> Vec<String> {
        self.failed.iter().map(|r| r.clone()).collect()
    }

    /// Clear all failed parsers (reset state).
    pub fn clear_all(&self) -> io::Result<()> {
        self.failed.clear();
        let path = self.failed_parsers_path();
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_new_registry_has_no_failed_parsers() {
        let temp = tempdir().unwrap();
        let registry = FailedParserRegistry::new(temp.path());
        registry.init().unwrap();

        assert!(!registry.is_failed("lua"));
        assert!(!registry.is_failed("rust"));
        assert!(registry.failed_parsers().is_empty());
    }

    #[test]
    fn test_mark_and_check_failed() {
        let temp = tempdir().unwrap();
        let registry = FailedParserRegistry::new(temp.path());
        registry.init().unwrap();

        registry.mark_failed("lua").unwrap();

        assert!(registry.is_failed("lua"));
        assert!(!registry.is_failed("rust"));
        assert_eq!(registry.failed_parsers(), vec!["lua"]);
    }

    #[test]
    fn test_clear_failed() {
        let temp = tempdir().unwrap();
        let registry = FailedParserRegistry::new(temp.path());
        registry.init().unwrap();

        registry.mark_failed("lua").unwrap();
        assert!(registry.is_failed("lua"));

        registry.clear_failed("lua").unwrap();
        assert!(!registry.is_failed("lua"));
    }

    #[test]
    fn test_failed_parsers_persist_across_restarts() {
        let temp = tempdir().unwrap();

        // First "session"
        {
            let registry = FailedParserRegistry::new(temp.path());
            registry.init().unwrap();
            registry.mark_failed("yaml").unwrap();
        }

        // Second "session" - should load persisted state
        {
            let registry = FailedParserRegistry::new(temp.path());
            registry.init().unwrap();
            assert!(registry.is_failed("yaml"));
        }
    }

    #[test]
    fn test_crash_detection_marks_parser_failed() {
        let temp = tempdir().unwrap();

        // Simulate a crash: begin_parsing but never end_parsing
        {
            let registry = FailedParserRegistry::new(temp.path());
            registry.init().unwrap();
            registry.begin_parsing("yaml").unwrap();
            // Simulated crash - no end_parsing() called
        }

        // Restart and init should detect the crash
        {
            let registry = FailedParserRegistry::new(temp.path());
            registry.init().unwrap();
            assert!(registry.is_failed("yaml"));
        }
    }

    #[test]
    fn test_successful_parsing_does_not_mark_failed() {
        let temp = tempdir().unwrap();

        // Normal parsing flow
        {
            let registry = FailedParserRegistry::new(temp.path());
            registry.init().unwrap();
            registry.begin_parsing("lua").unwrap();
            registry.end_parsing().unwrap();
        }

        // Restart should not see lua as failed
        {
            let registry = FailedParserRegistry::new(temp.path());
            registry.init().unwrap();
            assert!(!registry.is_failed("lua"));
        }
    }

    #[test]
    fn test_clear_all() {
        let temp = tempdir().unwrap();
        let registry = FailedParserRegistry::new(temp.path());
        registry.init().unwrap();

        registry.mark_failed("lua").unwrap();
        registry.mark_failed("rust").unwrap();

        registry.clear_all().unwrap();

        assert!(!registry.is_failed("lua"));
        assert!(!registry.is_failed("rust"));
        assert!(registry.failed_parsers().is_empty());
    }

    #[test]
    fn test_init_detects_crash_and_marks_failed() {
        let temp = tempdir().unwrap();

        // Simulate a crash: begin_parsing but never end_parsing
        {
            let registry = FailedParserRegistry::new(temp.path());
            registry.init().unwrap();
            registry.begin_parsing("zsh").unwrap();
            // Simulated crash - no end_parsing() called
        }

        // Restart and init should detect the crash
        {
            let registry = FailedParserRegistry::new(temp.path());
            registry.init().unwrap();
            // The crashed parser should be marked as failed
            assert!(registry.is_failed("zsh"));
        }
    }

    #[test]
    fn test_init_no_crash_no_failed_parsers() {
        let temp = tempdir().unwrap();

        // Normal startup - no crash
        let registry = FailedParserRegistry::new(temp.path());
        registry.init().unwrap();
        // No parsers should be marked as failed
        assert!(registry.failed_parsers().is_empty());
    }
}
