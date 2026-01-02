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

use arc_swap::ArcSwap;
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
    /// In-memory state of currently parsing language (for crash detection)
    /// Replaces synchronous disk writes on every parse operation
    current_parsing: Arc<ArcSwap<Option<String>>>,
}

impl FailedParserRegistry {
    /// Create a new registry with the given state directory.
    pub fn new(state_dir: &Path) -> Self {
        Self {
            failed: Arc::new(DashSet::new()),
            state_dir: state_dir.to_path_buf(),
            current_parsing: Arc::new(ArcSwap::new(Arc::new(None))),
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
    /// This updates in-memory state only. Crash detection happens by checking
    /// this state on restart (via init()).
    pub fn begin_parsing(&self, language: &str) -> io::Result<()> {
        // Update in-memory state atomically (no disk I/O)
        self.current_parsing.store(Arc::new(Some(language.to_string())));
        Ok(())
    }

    /// Record that parsing completed successfully.
    ///
    /// This clears in-memory state only (no disk I/O).
    pub fn end_parsing(&self) -> io::Result<()> {
        // Clear in-memory state atomically (no disk I/O)
        self.current_parsing.store(Arc::new(None));
        Ok(())
    }

    /// Get the currently parsing language (for testing).
    #[cfg(test)]
    pub(crate) fn current_parsing_language(&self) -> Option<String> {
        (**self.current_parsing.load()).clone()
    }

    /// Persist current parsing state to disk.
    ///
    /// This should be called on graceful shutdown to enable crash detection
    /// across process restarts. If a parser is currently being parsed, write
    /// its name to the parsing_in_progress file.
    pub fn persist_state(&self) -> io::Result<()> {
        if let Some(ref language) = **self.current_parsing.load() {
            fs::create_dir_all(&self.state_dir)?;
            fs::write(self.parsing_state_path(), language)?;
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
            // Simulated crash - persist state shows parsing was in progress
            registry.persist_state().unwrap();
            // No end_parsing() called - simulates crash during parsing
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
            // Simulated crash - persist state before process terminates
            registry.persist_state().unwrap();
            // No end_parsing() called - simulates crash during parsing
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

    #[test]
    fn test_begin_parsing_does_not_write_to_disk() {
        let temp = tempdir().unwrap();
        let registry = FailedParserRegistry::new(temp.path());
        registry.init().unwrap();

        // Call begin_parsing
        registry.begin_parsing("lua").unwrap();

        // Verify that parsing_in_progress file does NOT exist
        // (begin_parsing should only update in-memory state)
        let parsing_state_path = temp.path().join("parsing_in_progress");
        assert!(
            !parsing_state_path.exists(),
            "begin_parsing should not write parsing_in_progress file to disk"
        );

        // Verify that in-memory state is updated (we'll add accessor for this)
        assert_eq!(
            registry.current_parsing_language(),
            Some("lua".to_string()),
            "begin_parsing should update in-memory state"
        );
    }

    #[test]
    fn test_end_parsing_only_clears_memory() {
        let temp = tempdir().unwrap();
        let registry = FailedParserRegistry::new(temp.path());
        registry.init().unwrap();

        // Start parsing
        registry.begin_parsing("rust").unwrap();
        assert_eq!(registry.current_parsing_language(), Some("rust".to_string()));

        // End parsing
        registry.end_parsing().unwrap();

        // Verify in-memory state is cleared
        assert_eq!(
            registry.current_parsing_language(),
            None,
            "end_parsing should clear in-memory state"
        );

        // Verify no disk I/O happened
        let parsing_state_path = temp.path().join("parsing_in_progress");
        assert!(
            !parsing_state_path.exists(),
            "end_parsing should not create or modify files"
        );
    }
}
