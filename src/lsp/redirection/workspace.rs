//! Workspace setup for language server bridge connections.
//!
//! This module handles creating workspace structures for different project types:
//! - Cargo: Creates Cargo.toml and src/main.rs for Rust projects
//! - Generic: Creates a single virtual.<ext> file for other languages

use crate::config::settings::WorkspaceType;
use std::path::{Path, PathBuf};

/// Map language name to file extension.
///
/// Used when creating virtual files for Generic workspaces.
/// Returns a reasonable extension for common languages.
pub(crate) fn language_to_extension(language: &str) -> &'static str {
    match language.to_lowercase().as_str() {
        "rust" => "rs",
        "python" => "py",
        "javascript" => "js",
        "typescript" => "ts",
        "lua" => "lua",
        "go" => "go",
        "c" => "c",
        "cpp" | "c++" => "cpp",
        "java" => "java",
        "ruby" => "rb",
        "php" => "php",
        "swift" => "swift",
        "kotlin" => "kt",
        "scala" => "scala",
        "haskell" => "hs",
        "elixir" => "ex",
        "erlang" => "erl",
        "clojure" => "clj",
        "r" => "r",
        "julia" => "jl",
        "dart" => "dart",
        "vim" => "vim",
        "zig" => "zig",
        "ocaml" => "ml",
        "fsharp" | "f#" => "fs",
        "csharp" | "c#" => "cs",
        _ => "txt", // Default fallback
    }
}

/// Set up workspace files for a language server.
///
/// Creates the appropriate file structure based on workspace type:
/// - Cargo: Creates Cargo.toml and src/main.rs
/// - Generic: Creates virtual.<ext> file only
///
/// Returns the path to the virtual file that should be used for LSP operations.
pub fn setup_workspace(
    temp_dir: &Path,
    workspace_type: WorkspaceType,
    extension: &str,
) -> Option<PathBuf> {
    match workspace_type {
        WorkspaceType::Cargo => setup_cargo_workspace(temp_dir),
        WorkspaceType::Generic => setup_generic_workspace(temp_dir, extension),
    }
}

/// Set up a generic workspace with just a virtual file.
///
/// Creates a single virtual.<ext> file in the temp directory.
/// No project structure (Cargo.toml, package.json, etc.) is created.
fn setup_generic_workspace(temp_dir: &Path, extension: &str) -> Option<PathBuf> {
    let virtual_file = temp_dir.join(format!("virtual.{}", extension));
    std::fs::write(&virtual_file, "").ok()?;
    Some(virtual_file)
}

/// Set up workspace files with optional workspace type.
///
/// If workspace_type is None, defaults to Generic. Bridge servers should
/// explicitly specify workspace_type in their configuration if they need
/// a specific workspace structure (e.g., Cargo for rust-analyzer).
pub fn setup_workspace_with_option(
    temp_dir: &Path,
    workspace_type: Option<WorkspaceType>,
    extension: &str,
) -> Option<PathBuf> {
    let effective_type = workspace_type.unwrap_or(WorkspaceType::Generic);
    setup_workspace(temp_dir, effective_type, extension)
}

/// Set up a Cargo workspace with Cargo.toml and src/main.rs.
fn setup_cargo_workspace(temp_dir: &Path) -> Option<PathBuf> {
    let src_dir = temp_dir.join("src");
    std::fs::create_dir_all(&src_dir).ok()?;

    // Write minimal Cargo.toml
    let cargo_toml = temp_dir.join("Cargo.toml");
    std::fs::write(
        &cargo_toml,
        "[package]\nname = \"virtual\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .ok()?;

    // Create empty main.rs (will be overwritten by did_open)
    let main_rs = src_dir.join("main.rs");
    std::fs::write(&main_rs, "").ok()?;

    Some(main_rs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn setup_cargo_workspace_creates_cargo_toml_and_src_main_rs() {
        let temp = tempdir().unwrap();
        let temp_path = temp.path().to_path_buf();

        // Call setup_workspace with Cargo type
        let result = setup_workspace(&temp_path, WorkspaceType::Cargo, "rs");
        assert!(result.is_some(), "setup_workspace should succeed");

        let virtual_file = result.unwrap();

        // Verify Cargo.toml was created
        let cargo_toml = temp_path.join("Cargo.toml");
        assert!(cargo_toml.exists(), "Cargo.toml should exist");

        // Verify src/main.rs was created
        let main_rs = temp_path.join("src").join("main.rs");
        assert!(main_rs.exists(), "src/main.rs should exist");

        // Verify virtual_file points to src/main.rs
        assert_eq!(
            virtual_file, main_rs,
            "virtual_file should be src/main.rs for Cargo workspace"
        );
    }

    #[test]
    fn setup_workspace_with_option_none_defaults_to_generic() {
        let temp = tempdir().unwrap();
        let temp_path = temp.path().to_path_buf();

        // Call setup_workspace with None (should default to Generic behavior)
        let result = setup_workspace_with_option(&temp_path, None, "rs");
        assert!(result.is_some(), "setup_workspace should succeed");

        let virtual_file = result.unwrap();

        // Verify virtual.rs was created (Generic workspace)
        let expected_virtual_file = temp_path.join("virtual.rs");
        assert_eq!(
            virtual_file, expected_virtual_file,
            "virtual_file should be virtual.rs for None workspace_type (Generic default)"
        );
        assert!(
            expected_virtual_file.exists(),
            "virtual.rs should exist for None workspace_type"
        );

        // Verify NO Cargo.toml was created (no longer defaults to Cargo)
        let cargo_toml = temp_path.join("Cargo.toml");
        assert!(
            !cargo_toml.exists(),
            "Cargo.toml should NOT exist for None workspace_type (Generic default)"
        );

        // Verify NO src/ directory was created
        let src_dir = temp_path.join("src");
        assert!(
            !src_dir.exists(),
            "src/ directory should NOT exist for None workspace_type (Generic default)"
        );
    }

    #[test]
    fn setup_generic_workspace_creates_virtual_file_only() {
        let temp = tempdir().unwrap();
        let temp_path = temp.path().to_path_buf();

        // Call setup_workspace with Generic type and "py" extension
        let result = setup_workspace(&temp_path, WorkspaceType::Generic, "py");
        assert!(
            result.is_some(),
            "setup_workspace should succeed for Generic"
        );

        let virtual_file = result.unwrap();

        // Verify virtual.py was created
        let expected_virtual_file = temp_path.join("virtual.py");
        assert_eq!(
            virtual_file, expected_virtual_file,
            "virtual_file should be virtual.py for Generic workspace"
        );
        assert!(expected_virtual_file.exists(), "virtual.py should exist");

        // Verify NO Cargo.toml was created
        let cargo_toml = temp_path.join("Cargo.toml");
        assert!(
            !cargo_toml.exists(),
            "Cargo.toml should NOT exist for Generic workspace"
        );

        // Verify NO src/ directory was created
        let src_dir = temp_path.join("src");
        assert!(
            !src_dir.exists(),
            "src/ directory should NOT exist for Generic workspace"
        );
    }

    #[test]
    fn setup_generic_workspace_uses_extension_in_filename() {
        let temp = tempdir().unwrap();
        let temp_path = temp.path().to_path_buf();

        // Call setup_workspace with different extensions
        let result_ts = setup_workspace(&temp_path, WorkspaceType::Generic, "ts");
        assert!(result_ts.is_some());

        let virtual_file = result_ts.unwrap();
        assert_eq!(
            virtual_file,
            temp_path.join("virtual.ts"),
            "virtual_file should use the provided extension"
        );
    }

    #[test]
    fn spawn_with_generic_workspace_type_creates_virtual_file_not_cargo() {
        // This test verifies workspace setup behavior without spawning a real server.
        // We use setup_workspace_with_option directly to test the integration.
        let temp = tempdir().unwrap();
        let temp_path = temp.path().to_path_buf();

        // With Generic workspace_type, should create virtual.py not Cargo.toml
        let virtual_file =
            setup_workspace_with_option(&temp_path, Some(WorkspaceType::Generic), "py").unwrap();

        // Verify virtual.py was created
        assert!(
            temp_path.join("virtual.py").exists(),
            "virtual.py should exist for Generic workspace"
        );

        // Verify NO Cargo.toml was created
        assert!(
            !temp_path.join("Cargo.toml").exists(),
            "Cargo.toml should NOT exist for Generic workspace"
        );

        // Verify NO src/ directory was created
        assert!(
            !temp_path.join("src").exists(),
            "src/ directory should NOT exist for Generic workspace"
        );

        // Verify virtual_file path is correct
        assert_eq!(
            virtual_file,
            temp_path.join("virtual.py"),
            "virtual_file should be virtual.py for Generic workspace"
        );
    }
}
