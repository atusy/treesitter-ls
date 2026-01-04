//! End-to-end test for real lua-language-server initialization via bridge.
//!
//! This test verifies that BridgeConnection can spawn and initialize
//! a real lua-language-server process:
//! - Process spawns successfully
//! - Initialize request sent and InitializeResult received
//! - Initialized notification sent
//! - didOpen notification sent for virtual Lua document
//! - Full handshake completes within 5s timeout
//!
//! Run with: `cargo test --test e2e_bridge_init --features e2e`
//!
//! **Requirements**: lua-language-server must be installed and in PATH.
//! If not available, test will be skipped (not failed).

#![cfg(feature = "e2e")]

use std::time::{Duration, Instant};

// Import from src/lsp/bridge (internal module)
use treesitter_ls::lsp::bridge::connection::BridgeConnection;

#[tokio::test]
async fn test_lua_ls_initialization_handshake() {
    // Check if lua-language-server is available
    let check = tokio::process::Command::new("lua-language-server")
        .arg("--version")
        .output()
        .await;

    if check.is_err() {
        eprintln!("SKIP: lua-language-server not found in PATH");
        eprintln!("Install lua-language-server to run this test:");
        eprintln!("  brew install lua-language-server");
        return;
    }

    // Spawn lua-language-server
    let connection = BridgeConnection::new("lua-language-server")
        .await
        .expect("Failed to spawn lua-language-server");

    // Perform full initialization sequence with 5s timeout
    let start = Instant::now();
    let init_result = connection
        .initialize()
        .await
        .expect("Initialize handshake failed");
    let elapsed = start.elapsed();

    // Verify we got a valid InitializeResult
    assert!(
        init_result.get("capabilities").is_some(),
        "InitializeResult should have capabilities: {:?}",
        init_result
    );

    // Verify handshake completed quickly (should be < 5s, typically < 1s)
    assert!(
        elapsed < Duration::from_secs(5),
        "Initialization took {:?}, expected < 5s",
        elapsed
    );

    println!("✓ lua-language-server initialized in {:?}", elapsed);
}

#[tokio::test]
async fn test_lua_ls_did_open_after_initialization() {
    // Check if lua-language-server is available
    let check = tokio::process::Command::new("lua-language-server")
        .arg("--version")
        .output()
        .await;

    if check.is_err() {
        eprintln!("SKIP: lua-language-server not found in PATH");
        return;
    }

    // Spawn and initialize
    let connection = BridgeConnection::new("lua-language-server")
        .await
        .expect("Failed to spawn lua-language-server");

    connection
        .initialize()
        .await
        .expect("Initialize handshake failed");

    // Send didOpen for a virtual Lua document
    let did_open_result = connection
        .send_did_open("file:///virtual/test.lua", "lua", "local x = 42\nprint(x)")
        .await;

    assert!(
        did_open_result.is_ok(),
        "didOpen should succeed after initialization: {:?}",
        did_open_result.err()
    );

    println!("✓ didOpen sent successfully");
}

#[tokio::test]
async fn test_lua_ls_did_open_blocked_before_initialization() {
    // Check if lua-language-server is available
    let check = tokio::process::Command::new("lua-language-server")
        .arg("--version")
        .output()
        .await;

    if check.is_err() {
        eprintln!("SKIP: lua-language-server not found in PATH");
        return;
    }

    // Spawn but do NOT initialize
    let connection = BridgeConnection::new("lua-language-server")
        .await
        .expect("Failed to spawn lua-language-server");

    // Try to send didOpen before initialization
    let did_open_result = connection
        .send_did_open("file:///virtual/test.lua", "lua", "local x = 42")
        .await;

    assert!(
        did_open_result.is_err(),
        "didOpen should be blocked before initialization"
    );

    let error = did_open_result.unwrap_err();
    assert!(
        error.contains("SERVER_NOT_INITIALIZED"),
        "Error should mention SERVER_NOT_INITIALIZED: {}",
        error
    );
    assert!(
        error.contains("-32002"),
        "Error should include error code -32002: {}",
        error
    );

    println!("✓ Phase 1 guard correctly blocked didOpen before initialization");
}
