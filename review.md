# Code Review – `src/analysis/selection.rs`

## Issues

1. **Selection range results no longer align with requested positions (high)**
   - References: `src/analysis/selection.rs:1130-1172`, `src/analysis/selection.rs:1203-1235`
   - Both `handle_selection_range_with_injection` and `handle_selection_range_with_parsed_injection` now use `filter_map` to silently drop positions that cannot be mapped (e.g., stale cursors, temporarily missing tree). The LSP `textDocument/selectionRange` response must return one item per requested position in the same order so clients can associate each result with its cursor. By shortening the vector, the server causes every remaining selection to shift toward earlier cursors—position #3 receives the selection that actually belongs to position #4, etc.—producing incorrect behavior in all multi-cursor editors. Instead of omitting entries, keep the response aligned (e.g., emit a fallback range or `None` for the failed position, or abort the whole request) so the lengths always match.
