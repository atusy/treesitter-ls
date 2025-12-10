
# Review: `src/analysis/selection.rs`

## Issue 1 – Injected ranges report byte columns to the LSP (High)
- **Location:** `src/analysis/selection.rs:606` and `src/analysis/selection.rs:669`
- `build_injected_selection_range` converts injected AST nodes into `SelectionRange`s via `adjust_range_to_host`, but that helper just adds two tree‑sitter `Point`s together and wraps the raw byte counts in `tower_lsp::Position`.
- Tree‑sitter columns are measured in UTF‑8 bytes while the LSP requires UTF‑16 code units. For any host or injected snippet containing multi‑byte characters (e.g., `let yaml = r#"あ: 0"#;`), the `SelectionRange.range.start.character` that we return to the client is larger than it should be, so selection highlighting jumps to the wrong column. Because the host chain built with `node_to_range` *does* use the UTF‑16 mapper, `skip_to_distinct_host` compares ranges expressed in different units and may fail to connect the injected hierarchy back to its host when non‑ASCII characters appear before the injection.
- **Fix:** Thread a `PositionMapper` (or another byte→UTF‑16 conversion helper) down into `build_injected_selection_range`/`adjust_range_to_host` so that every injected node is converted through the same path as host nodes before we send it to the LSP or compare it to host ranges.
- **Tests:** Add an integration test that parses an injected snippet preceded by multi‑byte characters (e.g., Rust raw string containing Japanese text treated as YAML) and assert that `handle_selection_range_with_parsed_injection` reports UTF‑16 columns and still attaches the injected hierarchy to the host chain.

## Issue 2 – Offset handling reinterprets UTF‑16 columns as bytes (High)
- **Location:** `src/analysis/selection.rs:301`
- When an injection capture uses `#offset`, we trim the text via `calculate_effective_range_with_text` and then compute `effective_start_position` by calling `mapper.byte_to_position(...)` followed by `tree_sitter::Point::new(p.line as usize, p.character as usize)`.
- `byte_to_position` returns UTF‑16 columns, but `Point::new` expects byte columns. If any multi‑byte character appears before the injection, `effective_start_position.column` becomes too small and all subsequent calculations that rely on it (`adjust_range_to_host`, nested injections, offset fences) shift the injected ranges to the left. The bug is especially visible in Markdown front‑matter, where offsets are the norm.
- **Fix:** Stop round‑tripping through UTF‑16 when we need a tree‑sitter `Point`. Either compute the byte column directly from the mapper’s internal `LineIndex`, or introduce a dedicated helper (e.g., `byte_to_point`) that returns UTF‑8 coordinates.
- **Tests:** Extend the offset regression tests to cover a document with multi‑byte characters before the fenced block so that the failure is caught automatically.

## Issue 3 – ASCII‑only helpers still drive incremental edits (High)
- **Location:** `src/analysis/selection.rs:10` and usage in `src/lsp/lsp_impl.rs:412`
- The `position_to_point`/`point_to_position` helpers simply copy the numeric line/column values between `Position` and `tree_sitter::Point`, even though the doc comment states “Only use for ASCII‑only contexts.” These functions remain `pub` and `position_to_point` is currently used when constructing `tree_sitter::InputEdit` during `textDocument/didChange`.
- When a user edits a document containing multi‑byte characters, the `start_position`/`old_end_position` stored in each `InputEdit` uses UTF‑16 columns instead of byte columns, so tree‑sitter receives corrupt edit coordinates and either mis‑parses or rejects the incremental edit. That is a data‑loss level bug caused by the helpers defined in this file.
- **Fix:** Either remove these helpers or reimplement them so they convert through `PositionMapper` (or another byte↔UTF‑16 conversion utility) before exposing them publicly. The LSP layer should not call them until they are corrected.
- **Tests:** Add an incremental‑edit integration test that edits a document containing a non‑ASCII character and asserts that tree‑sitter still receives consistent ranges.
