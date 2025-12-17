// CLI integration tests for treesitter-ls
// Tests the command-line interface functionality

use std::process::Command;

/// Test that --help flag shows help message with program description
#[test]
fn test_help_flag_shows_help_message() {
    let output = Command::new(env!("CARGO_BIN_EXE_treesitter-ls"))
        .arg("--help")
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should exit successfully
    assert!(output.status.success(), "Help should exit with success");

    // Should contain program name
    assert!(
        stdout.contains("treesitter-ls"),
        "Help should contain program name. Got: {}",
        stdout
    );

    // Should contain some description
    assert!(
        stdout.contains("Language Server") || stdout.contains("Tree-sitter"),
        "Help should contain description. Got: {}",
        stdout
    );

    // Should show language subcommand (hierarchical CLI)
    assert!(
        stdout.contains("language"),
        "Help should show language subcommand. Got: {}",
        stdout
    );
}

/// Test that language install --help shows usage with LANGUAGE argument
#[test]
fn test_install_help_shows_language_argument() {
    let output = Command::new(env!("CARGO_BIN_EXE_treesitter-ls"))
        .args(["language", "install", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should exit successfully
    assert!(
        output.status.success(),
        "Install help should exit with success"
    );

    // Should contain LANGUAGE or language reference
    assert!(
        stdout.to_lowercase().contains("language"),
        "Install help should mention language argument. Got: {}",
        stdout
    );
}

/// Test that language install command with unsupported language shows helpful error
#[test]
fn test_install_command_unsupported_language_shows_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_treesitter-ls"))
        .args([
            "language",
            "install",
            "nonexistent_language_xyz",
            "--data-dir",
            "/tmp/test-cli",
        ])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should exit with failure for unsupported language
    assert!(
        !output.status.success(),
        "Install should exit with failure for unsupported language"
    );

    // Should contain error message about the language not being found
    assert!(
        stderr.to_lowercase().contains("not found")
            || stderr.to_lowercase().contains("not supported")
            || stderr.to_lowercase().contains("failed"),
        "Install should print helpful error for unsupported language. Got: {}",
        stderr
    );
}

/// Test that running with no arguments would start LSP server
/// (We can't fully test LSP startup without a proper client, but we can verify
/// the binary starts without errors when given empty stdin)
#[test]
fn test_no_args_does_not_show_help() {
    // When run with no args, it should NOT print help (it should try to start LSP)
    // We use timeout to prevent hanging on stdin read
    let output = Command::new(env!("CARGO_BIN_EXE_treesitter-ls"))
        .stdin(std::process::Stdio::null())
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should NOT contain help output (would indicate wrong behavior)
    assert!(
        !stdout.contains("Usage:") || stdout.is_empty(),
        "No args should not print help (should try to start LSP). Got: {}",
        stdout
    );
}

/// Test that --version flag works
#[test]
fn test_version_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_treesitter-ls"))
        .arg("--version")
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should exit successfully
    assert!(output.status.success(), "Version should exit with success");

    // Should contain version number pattern
    assert!(
        stdout.contains("treesitter-ls") || stdout.contains("0."),
        "Version should show program name or version. Got: {}",
        stdout
    );
}

/// Test that language --help shows available actions
#[test]
fn test_language_help_shows_actions() {
    let output = Command::new(env!("CARGO_BIN_EXE_treesitter-ls"))
        .args(["language", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should exit successfully
    assert!(
        output.status.success(),
        "Language help should exit with success"
    );

    // Should contain install and list actions
    assert!(
        stdout.contains("install"),
        "Language help should show install action. Got: {}",
        stdout
    );
    assert!(
        stdout.contains("list"),
        "Language help should show list action. Got: {}",
        stdout
    );
}

/// Test that language list command works
#[test]
fn test_language_list_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_treesitter-ls"))
        .args(["language", "list"])
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should exit successfully
    assert!(
        output.status.success(),
        "Language list should exit with success. stderr: {}",
        stderr
    );

    // Should contain some common languages
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        combined.contains("lua") || combined.contains("rust") || combined.contains("python"),
        "Language list should show some languages. Got: {}",
        combined
    );
}

/// Test that config --help shows available actions
#[test]
fn test_config_help_shows_actions() {
    let output = Command::new(env!("CARGO_BIN_EXE_treesitter-ls"))
        .args(["config", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should exit successfully
    assert!(
        output.status.success(),
        "Config help should exit with success"
    );

    // Should contain init action
    assert!(
        stdout.contains("init"),
        "Config help should show init action. Got: {}",
        stdout
    );
}

/// Test that config init creates a configuration file
#[test]
fn test_config_init_creates_file() {
    use std::fs;
    use std::path::Path;

    let test_dir = "/tmp/test-config-init";
    let config_path = Path::new(test_dir).join("treesitter-ls.toml");

    // Clean up before test
    let _ = fs::remove_dir_all(test_dir);
    fs::create_dir_all(test_dir).expect("Failed to create test directory");

    let output = Command::new(env!("CARGO_BIN_EXE_treesitter-ls"))
        .args(["config", "init"])
        .current_dir(test_dir)
        .output()
        .expect("Failed to execute command");

    // Should exit successfully
    assert!(
        output.status.success(),
        "Config init should exit with success. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // File should exist
    assert!(
        config_path.exists(),
        "Config file should be created at {}",
        config_path.display()
    );

    // File should contain expected options
    let content = fs::read_to_string(&config_path).expect("Failed to read config");
    assert!(
        content.contains("autoInstall") && content.contains("searchPaths"),
        "Config should contain expected options. Got: {}",
        content
    );

    // Clean up
    let _ = fs::remove_dir_all(test_dir);
}

/// Test that config init does not overwrite existing file without --force
#[test]
fn test_config_init_no_overwrite_without_force() {
    use std::fs;
    use std::path::Path;

    let test_dir = "/tmp/test-config-no-overwrite";
    let config_path = Path::new(test_dir).join("treesitter-ls.toml");

    // Clean up and setup
    let _ = fs::remove_dir_all(test_dir);
    fs::create_dir_all(test_dir).expect("Failed to create test directory");
    fs::write(&config_path, "existing").expect("Failed to write existing config");

    let output = Command::new(env!("CARGO_BIN_EXE_treesitter-ls"))
        .args(["config", "init"])
        .current_dir(test_dir)
        .output()
        .expect("Failed to execute command");

    // Should exit with failure
    assert!(
        !output.status.success(),
        "Config init should fail when file exists"
    );

    // Original content should be preserved
    let content = fs::read_to_string(&config_path).expect("Failed to read config");
    assert_eq!(content, "existing", "Original content should be preserved");

    // Clean up
    let _ = fs::remove_dir_all(test_dir);
}

/// Test that config init --force overwrites existing file
#[test]
fn test_config_init_force_overwrites() {
    use std::fs;
    use std::path::Path;

    let test_dir = "/tmp/test-config-force";
    let config_path = Path::new(test_dir).join("treesitter-ls.toml");

    // Clean up and setup
    let _ = fs::remove_dir_all(test_dir);
    fs::create_dir_all(test_dir).expect("Failed to create test directory");
    fs::write(&config_path, "existing").expect("Failed to write existing config");

    let output = Command::new(env!("CARGO_BIN_EXE_treesitter-ls"))
        .args(["config", "init", "--force"])
        .current_dir(test_dir)
        .output()
        .expect("Failed to execute command");

    // Should exit successfully
    assert!(
        output.status.success(),
        "Config init --force should exit with success"
    );

    // Content should be replaced
    let content = fs::read_to_string(&config_path).expect("Failed to read config");
    assert!(
        !content.contains("existing"),
        "Content should be overwritten"
    );
    assert!(
        content.contains("autoInstall"),
        "New content should be present"
    );

    // Clean up
    let _ = fs::remove_dir_all(test_dir);
}
