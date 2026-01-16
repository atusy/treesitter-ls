use kakehashi::text::PositionMapper;
/// Tests for incremental edit handling with multi-byte characters
///
/// This test verifies that InputEdit positions are computed correctly for documents
/// containing multi-byte UTF-8 characters. LSP uses UTF-16 code units for positions,
/// while tree-sitter uses byte offsets.
use tree_sitter::{InputEdit, Parser, Point};

/// Test that position_to_point correctly converts UTF-16 columns to byte columns
///
/// Given: A line with Japanese text "あいう" (3 hiragana chars, each 3 bytes in UTF-8)
/// When: We have an LSP position at UTF-16 column 3 (after "あいう")
/// Then: The tree-sitter Point should have column 9 (3 chars × 3 bytes)
#[test]
fn test_position_to_point_with_multibyte_chars() {
    // "あいう" = 3 hiragana characters
    // Each is 3 bytes in UTF-8, 1 code unit in UTF-16
    // After "あいう" and "x": UTF-16 column 4, byte column 10
    let text = "あいうx\n";

    let mapper = PositionMapper::new(text);

    // Position at "x" (after 3 Japanese chars)
    // UTF-16: line 0, character 3 (after 3 UTF-16 code units)
    // Byte: line 0, column 9 (after 9 bytes: 3×3)
    let lsp_position = tower_lsp::lsp_types::Position::new(0, 3);

    let point = mapper.position_to_point(lsp_position);
    assert!(point.is_some(), "Should be able to convert position");

    let point = point.unwrap();
    assert_eq!(point.row, 0, "Row should be 0");
    assert_eq!(
        point.column, 9,
        "Column should be 9 bytes (3 hiragana × 3 bytes each), got {}",
        point.column
    );
}

/// Test that InputEdit works correctly after applying the fix
#[test]
fn test_incremental_edit_with_multibyte_preserves_parsing() {
    // Document with Japanese text
    let original = "let x = \"あいう\";\n";

    let mut parser = Parser::new();
    let rust_lang = tree_sitter_rust::LANGUAGE.into();
    parser.set_language(&rust_lang).unwrap();

    // Parse original document
    let tree = parser.parse(original, None).unwrap();
    assert!(
        !tree.root_node().has_error(),
        "Original should parse without errors"
    );

    // Now simulate an edit: change "あいう" to "xyz"
    // The string content starts at byte 9 (after `let x = "`)
    // "あいう" is 9 bytes (3 chars × 3 bytes each)
    let new_text = "let x = \"xyz\";\n";

    // Calculate edit using PositionMapper (simulating what lsp_impl.rs should do)
    let mapper = PositionMapper::new(original);

    // The edit range in LSP terms (UTF-16):
    // Start: line 0, character 9 (after `let x = "`)
    // End: line 0, character 12 (after `let x = "あいう`)
    //
    // In UTF-16: "あいう" is 3 code units (each hiragana is 1 UTF-16 code unit)
    // But wait - the prefix `let x = "` is 9 ASCII chars
    let start_position = tower_lsp::lsp_types::Position::new(0, 9);
    let end_position = tower_lsp::lsp_types::Position::new(0, 12);

    // Get byte offsets
    let start_byte = mapper.position_to_byte(start_position).unwrap();
    let old_end_byte = mapper.position_to_byte(end_position).unwrap();

    assert_eq!(start_byte, 9, "Start byte should be 9 (after `let x = \"`)");
    assert_eq!(
        old_end_byte, 18,
        "End byte should be 18 (9 + 9 bytes for あいう)"
    );

    // Get tree-sitter Points using the NEW method
    let start_point = mapper.position_to_point(start_position).unwrap();
    let old_end_point = mapper.position_to_point(end_position).unwrap();

    // Verify Points have byte columns, not UTF-16 columns
    assert_eq!(start_point.column, 9, "Start column should be 9 bytes");
    assert_eq!(
        old_end_point.column, 18,
        "End column should be 18 bytes, got {}",
        old_end_point.column
    );

    // Create InputEdit with correct byte-based Points
    let edit = InputEdit {
        start_byte,
        old_end_byte,
        new_end_byte: start_byte + 3, // "xyz" is 3 bytes
        start_position: start_point,
        old_end_position: old_end_point,
        new_end_position: Point::new(0, start_byte + 3), // column is byte-based
    };

    // Apply edit to tree
    let mut edited_tree = tree;
    edited_tree.edit(&edit);

    // Re-parse with edit information
    let new_tree = parser.parse(new_text, Some(&edited_tree)).unwrap();

    // The tree should parse correctly without errors
    assert!(
        !new_tree.root_node().has_error(),
        "Edited document should parse without errors"
    );

    // Verify the structure is correct
    let root = new_tree.root_node();
    assert_eq!(root.kind(), "source_file");

    // Find the string literal
    let let_decl = root.child(0).expect("Should have a statement");
    assert_eq!(let_decl.kind(), "let_declaration");
}

/// Test that a naive position_to_point (treating UTF-16 as bytes) would cause issues
/// This test documents the bug behavior that was previously in the codebase
#[test]
fn test_old_position_to_point_is_incorrect_for_multibyte() {
    use tower_lsp::lsp_types::Position;
    use tree_sitter::Point;

    // This is the BUGGY implementation that was previously exported.
    // It's now restricted to tests only and not publicly accessible.
    fn buggy_position_to_point(pos: &Position) -> Point {
        Point::new(pos.line as usize, pos.character as usize)
    }

    let text = "あいう";
    // After "あいう" in UTF-16: column 3
    // After "あいう" in bytes: column 9

    let lsp_position = Position::new(0, 3);

    // The BUGGY function just copies the numeric value - this is WRONG
    let old_point = buggy_position_to_point(&lsp_position);

    // This demonstrates the bug: old_point.column is 3 (UTF-16), not 9 (bytes)
    assert_eq!(
        old_point.column, 3,
        "Buggy function incorrectly uses UTF-16 column as byte column"
    );

    // The CORRECT value should be 9
    let mapper = PositionMapper::new(text);
    let correct_point = mapper.position_to_point(lsp_position).unwrap();
    assert_eq!(correct_point.column, 9, "Correct column should be 9 bytes");

    // They differ!
    assert_ne!(
        old_point.column, correct_point.column,
        "This demonstrates the bug: naive function gives wrong result for multi-byte chars"
    );
}
