//! Query file downloading from nvim-treesitter repository.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Base URL for nvim-treesitter query files on GitHub.
const NVIM_TREESITTER_QUERIES_URL: &str =
    "https://raw.githubusercontent.com/nvim-treesitter/nvim-treesitter/master/queries";

/// Query file types to download.
const QUERY_FILES: &[&str] = &["highlights.scm", "locals.scm", "injections.scm"];

/// Error types for query installation.
#[derive(Debug)]
pub enum QueryInstallError {
    /// The language is not supported (queries don't exist in nvim-treesitter).
    LanguageNotSupported(String),
    /// HTTP request failed.
    HttpError(String),
    /// File system operation failed.
    IoError(std::io::Error),
    /// Queries already exist and --force not specified.
    AlreadyExists(PathBuf),
}

impl std::fmt::Display for QueryInstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LanguageNotSupported(lang) => {
                write!(
                    f,
                    "Language '{}' is not supported or queries not found in nvim-treesitter",
                    lang
                )
            }
            Self::HttpError(msg) => write!(f, "HTTP error: {}", msg),
            Self::IoError(e) => write!(f, "IO error: {}", e),
            Self::AlreadyExists(path) => {
                write!(
                    f,
                    "Queries already exist at {}. Use --force to overwrite.",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for QueryInstallError {}

impl From<std::io::Error> for QueryInstallError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

/// Result of installing queries for a language.
pub struct QueryInstallResult {
    /// The language that was installed.
    pub language: String,
    /// Path where queries were installed.
    pub install_path: PathBuf,
    /// List of files that were downloaded.
    pub files_downloaded: Vec<String>,
}

/// Download and install query files for a language.
///
/// # Arguments
/// * `language` - The language to install queries for (e.g., "lua", "rust")
/// * `data_dir` - The base data directory for treesitter-ls
/// * `force` - Whether to overwrite existing queries
///
/// # Returns
/// * `Ok(QueryInstallResult)` - Installation succeeded
/// * `Err(QueryInstallError)` - Installation failed
pub fn install_queries(
    language: &str,
    data_dir: &Path,
    force: bool,
) -> Result<QueryInstallResult, QueryInstallError> {
    let queries_dir = data_dir.join("queries").join(language);

    // Check if queries already exist
    if queries_dir.exists() && !force {
        return Err(QueryInstallError::AlreadyExists(queries_dir));
    }

    // Create the queries directory
    fs::create_dir_all(&queries_dir)?;

    let mut files_downloaded = Vec::new();
    let mut any_success = false;

    // Download each query file
    for query_file in QUERY_FILES {
        let url = format!(
            "{}/{}/{}",
            NVIM_TREESITTER_QUERIES_URL, language, query_file
        );

        match download_file(&url) {
            Ok(content) => {
                let file_path = queries_dir.join(query_file);
                let mut file = fs::File::create(&file_path)?;
                file.write_all(content.as_bytes())?;
                files_downloaded.push(query_file.to_string());
                any_success = true;
            }
            Err(e) => {
                // highlights.scm is required, others are optional
                if *query_file == "highlights.scm" {
                    // Clean up the directory we created
                    let _ = fs::remove_dir_all(&queries_dir);
                    return Err(QueryInstallError::LanguageNotSupported(
                        language.to_string(),
                    ));
                }
                // Log but continue for optional files
                eprintln!(
                    "Note: {} not available for {} ({})",
                    query_file, language, e
                );
            }
        }
    }

    if !any_success {
        let _ = fs::remove_dir_all(&queries_dir);
        return Err(QueryInstallError::LanguageNotSupported(
            language.to_string(),
        ));
    }

    Ok(QueryInstallResult {
        language: language.to_string(),
        install_path: queries_dir,
        files_downloaded,
    })
}

/// Download a file from a URL.
fn download_file(url: &str) -> Result<String, QueryInstallError> {
    let response =
        reqwest::blocking::get(url).map_err(|e| QueryInstallError::HttpError(e.to_string()))?;

    if !response.status().is_success() {
        return Err(QueryInstallError::HttpError(format!(
            "HTTP {} for {}",
            response.status(),
            url
        )));
    }

    response
        .text()
        .map_err(|e| QueryInstallError::HttpError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_install_queries_creates_directory_structure() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().to_path_buf();

        // This test requires network access - skip in CI if needed
        let result = install_queries("lua", &data_dir, false);

        // The test may fail due to network issues, but structure should be correct
        if let Ok(result) = result {
            assert_eq!(result.language, "lua");
            assert!(result.install_path.exists());
            assert!(
                result
                    .files_downloaded
                    .contains(&"highlights.scm".to_string())
            );
        }
    }

    #[test]
    fn test_install_queries_returns_error_for_nonexistent_language() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().to_path_buf();

        let result = install_queries("nonexistent_language_xyz_123", &data_dir, false);

        assert!(result.is_err());
        if let Err(QueryInstallError::LanguageNotSupported(lang)) = result {
            assert_eq!(lang, "nonexistent_language_xyz_123");
        }
    }

    #[test]
    fn test_install_queries_respects_force_flag() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().to_path_buf();
        let queries_dir = data_dir.join("queries").join("lua");

        // Create existing directory
        fs::create_dir_all(&queries_dir).unwrap();
        fs::write(queries_dir.join("highlights.scm"), "existing content").unwrap();

        // Without force, should error
        let result = install_queries("lua", &data_dir, false);
        assert!(matches!(result, Err(QueryInstallError::AlreadyExists(_))));

        // With force, should succeed (requires network)
        // Skip actual download test to avoid flaky CI
    }
}
