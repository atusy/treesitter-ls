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
