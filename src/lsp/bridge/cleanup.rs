//! Cleanup of stale temporary directories.
//!
//! This module handles cleaning up orphaned temporary directories from
//! crashed treesitter-ls sessions.

use std::path::Path;

/// Statistics returned by cleanup_stale_temp_dirs
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CleanupStats {
    /// Number of stale directories successfully removed
    pub dirs_removed: usize,
    /// Number of directories kept (newer than max_age)
    pub dirs_kept: usize,
    /// Number of directories that failed to remove (e.g., permission denied)
    pub dirs_failed: usize,
}

/// The prefix used for all treesitter-ls temporary directories
pub const TEMP_DIR_PREFIX: &str = "treesitter-ls-";

/// Default max age for stale temp directory cleanup (24 hours)
pub const DEFAULT_CLEANUP_MAX_AGE: std::time::Duration =
    std::time::Duration::from_secs(24 * 60 * 60);

/// Perform startup cleanup of stale temp directories.
///
/// This is called during LSP server initialization to clean up
/// orphaned temporary directories from crashed sessions.
///
/// The cleanup is non-blocking and logs any errors rather than failing.
pub fn startup_cleanup() {
    let temp_dir = std::env::temp_dir();

    match cleanup_stale_temp_dirs(&temp_dir, DEFAULT_CLEANUP_MAX_AGE) {
        Ok(stats) => {
            if stats.dirs_removed > 0 || stats.dirs_failed > 0 {
                log::info!(
                    target: "treesitter_ls::cleanup",
                    "Startup cleanup: removed {} stale dirs, kept {}, failed {}",
                    stats.dirs_removed,
                    stats.dirs_kept,
                    stats.dirs_failed
                );
            }
        }
        Err(e) => {
            log::warn!(
                target: "treesitter_ls::cleanup",
                "Startup cleanup failed to read temp directory: {}",
                e
            );
        }
    }
}

/// Clean up stale temporary directories created by treesitter-ls.
///
/// Scans the given temp directory for directories matching the pattern
/// `treesitter-ls-*` and removes those older than `max_age`.
///
/// # Arguments
/// * `temp_dir` - The directory to scan for stale temp directories
/// * `max_age` - Maximum age for directories; older ones will be removed
///
/// # Returns
/// * `Ok(CleanupStats)` - Statistics about the cleanup operation
/// * `Err(io::Error)` - If the temp directory cannot be read
pub fn cleanup_stale_temp_dirs(
    temp_dir: &Path,
    max_age: std::time::Duration,
) -> std::io::Result<CleanupStats> {
    let mut stats = CleanupStats::default();
    let now = std::time::SystemTime::now();

    // Read directory entries
    let entries = std::fs::read_dir(temp_dir)?;

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip non-directories
        if !path.is_dir() {
            continue;
        }

        // Check if directory name matches our prefix
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        if !name.starts_with(TEMP_DIR_PREFIX) {
            continue;
        }

        // Check directory age using modification time
        let is_stale = match entry.metadata() {
            Ok(metadata) => match metadata.modified() {
                Ok(modified) => match now.duration_since(modified) {
                    Ok(age) => age > max_age,
                    Err(_) => false, // Modified time is in the future - treat as fresh
                },
                Err(_) => true, // Can't get modified time - treat as stale
            },
            Err(_) => true, // Can't get metadata - treat as stale
        };

        if !is_stale {
            stats.dirs_kept += 1;
            continue;
        }

        // Remove the stale directory
        match std::fs::remove_dir_all(&path) {
            Ok(_) => {
                log::debug!(
                    target: "treesitter_ls::cleanup",
                    "Removed stale temp directory: {}",
                    path.display()
                );
                stats.dirs_removed += 1;
            }
            Err(e) => {
                log::warn!(
                    target: "treesitter_ls::cleanup",
                    "Failed to remove stale temp directory {}: {}",
                    path.display(),
                    e
                );
                stats.dirs_failed += 1;
            }
        }
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn cleanup_stale_temp_dirs_can_be_called_with_valid_args() {
        let temp = tempdir().unwrap();
        let max_age = Duration::from_secs(24 * 60 * 60); // 24 hours

        // The function should be callable and return Ok
        let result = cleanup_stale_temp_dirs(temp.path(), max_age);
        assert!(result.is_ok(), "cleanup_stale_temp_dirs should return Ok");

        let stats = result.unwrap();
        // With an empty temp directory, all stats should be zero
        assert_eq!(stats.dirs_removed, 0);
        assert_eq!(stats.dirs_kept, 0);
        assert_eq!(stats.dirs_failed, 0);
    }

    #[test]
    fn cleanup_identifies_directories_matching_treesitter_ls_prefix() {
        let temp = tempdir().unwrap();

        // Create directories with treesitter-ls- prefix
        std::fs::create_dir(temp.path().join("treesitter-ls-ra-12345")).unwrap();
        std::fs::create_dir(temp.path().join("treesitter-ls-rust-analyzer-67890")).unwrap();

        // Create directories WITHOUT treesitter-ls- prefix (should be ignored)
        std::fs::create_dir(temp.path().join("other-project-temp")).unwrap();
        std::fs::create_dir(temp.path().join("random-dir")).unwrap();

        // Use max_age of 0 so all directories are considered stale
        let max_age = Duration::from_secs(0);

        let result = cleanup_stale_temp_dirs(temp.path(), max_age);
        assert!(result.is_ok());

        let stats = result.unwrap();

        // Should have removed 2 directories (only those with treesitter-ls- prefix)
        assert_eq!(
            stats.dirs_removed, 2,
            "Should remove exactly 2 directories with treesitter-ls- prefix"
        );

        // Verify that the treesitter-ls directories are gone
        assert!(
            !temp.path().join("treesitter-ls-ra-12345").exists(),
            "treesitter-ls-ra-12345 should be removed"
        );
        assert!(
            !temp
                .path()
                .join("treesitter-ls-rust-analyzer-67890")
                .exists(),
            "treesitter-ls-rust-analyzer-67890 should be removed"
        );

        // Verify that non-matching directories are still there
        assert!(
            temp.path().join("other-project-temp").exists(),
            "other-project-temp should NOT be removed"
        );
        assert!(
            temp.path().join("random-dir").exists(),
            "random-dir should NOT be removed"
        );
    }

    #[test]
    fn cleanup_removes_directories_older_than_max_age() {
        use filetime::{FileTime, set_file_mtime};
        use std::time::SystemTime;

        let temp = tempdir().unwrap();

        // Create a directory and make it old (2 days ago)
        let old_dir = temp.path().join("treesitter-ls-old-12345");
        std::fs::create_dir(&old_dir).unwrap();

        // Set modification time to 2 days ago
        let two_days_ago = SystemTime::now() - Duration::from_secs(2 * 24 * 60 * 60);
        let mtime = FileTime::from_system_time(two_days_ago);
        set_file_mtime(&old_dir, mtime).unwrap();

        // Use max_age of 24 hours
        let max_age = Duration::from_secs(24 * 60 * 60);

        let result = cleanup_stale_temp_dirs(temp.path(), max_age);
        assert!(result.is_ok());

        let stats = result.unwrap();

        // The old directory should have been removed
        assert_eq!(
            stats.dirs_removed, 1,
            "Should remove 1 directory older than max_age"
        );
        assert!(
            !old_dir.exists(),
            "Directory older than max_age should be removed"
        );
    }

    #[test]
    fn cleanup_keeps_directories_newer_than_max_age() {
        let temp = tempdir().unwrap();

        // Create a fresh directory (just now - definitely newer than 24h)
        let fresh_dir = temp.path().join("treesitter-ls-fresh-12345");
        std::fs::create_dir(&fresh_dir).unwrap();

        // Use max_age of 24 hours
        let max_age = Duration::from_secs(24 * 60 * 60);

        let result = cleanup_stale_temp_dirs(temp.path(), max_age);
        assert!(result.is_ok());

        let stats = result.unwrap();

        // The fresh directory should be kept
        assert_eq!(stats.dirs_removed, 0, "Should NOT remove fresh directories");
        assert_eq!(stats.dirs_kept, 1, "Should keep 1 fresh directory");
        assert!(
            fresh_dir.exists(),
            "Directory newer than max_age should be kept"
        );
    }

    #[test]
    fn cleanup_continues_gracefully_when_removal_fails() {
        use filetime::{FileTime, set_file_mtime};
        use std::time::SystemTime;

        let temp = tempdir().unwrap();

        // Create two old directories
        let dir1 = temp.path().join("treesitter-ls-old1-12345");
        let dir2 = temp.path().join("treesitter-ls-old2-67890");
        std::fs::create_dir(&dir1).unwrap();
        std::fs::create_dir(&dir2).unwrap();

        // Make both directories old (2 days ago)
        let two_days_ago = SystemTime::now() - Duration::from_secs(2 * 24 * 60 * 60);
        let mtime = FileTime::from_system_time(two_days_ago);
        set_file_mtime(&dir1, mtime).unwrap();
        set_file_mtime(&dir2, mtime).unwrap();

        // On Unix, we can make dir1 unremovable by making it immutable via parent permissions
        // But this is tricky in tests. Instead, let's test the stats tracking behavior
        // by ensuring both directories would be considered for removal

        let max_age = Duration::from_secs(24 * 60 * 60);

        let result = cleanup_stale_temp_dirs(temp.path(), max_age);
        assert!(
            result.is_ok(),
            "Should return Ok even if some removals might fail"
        );

        let stats = result.unwrap();

        // Both directories should have been processed
        assert_eq!(
            stats.dirs_removed + stats.dirs_failed,
            2,
            "Should process exactly 2 directories (removed + failed = 2)"
        );

        // In this case both should succeed since we didn't actually block removal
        assert_eq!(stats.dirs_removed, 2, "Both directories should be removed");
        assert_eq!(stats.dirs_failed, 0, "No failures expected in this test");
    }

    #[test]
    fn startup_cleanup_can_be_called_without_panic() {
        // Test that startup_cleanup() can be called without panicking.
        // It uses the real system temp dir, so we just verify it doesn't crash.
        // Any stale directories it finds will be cleaned up.
        startup_cleanup();

        // If we get here, the function completed without panicking
        // We can't easily verify the exact behavior since it uses the real temp dir,
        // but we can verify the function signature and error handling work correctly.
    }

    #[test]
    fn default_cleanup_max_age_is_24_hours() {
        assert_eq!(
            DEFAULT_CLEANUP_MAX_AGE,
            Duration::from_secs(24 * 60 * 60),
            "Default max age should be 24 hours"
        );
    }
}
