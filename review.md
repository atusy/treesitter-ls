# Code Review – `src/analysis/selection.rs`

## Issues

1. **Offset slicing can panic when queries push ranges outside the buffer (critical)**
   - References: `src/analysis/selection.rs:301-309`, `src/analysis/selection.rs:472-504`, `src/analysis/selection.rs:514-517`
   - `calculate_effective_range_with_text` returns raw byte offsets with no clamping. When an injection query supplies positive offsets that extend beyond the captured node—e.g., `#offset! @injection.content 0 0 0 1` on a capture that already ends at EOF—the subsequent slices `&text[effective.start..effective.end]` in the blocks above will panic with “byte index out of bounds”. Queries are loaded from workspace files, so a malformed or malicious query crashes the whole server. Guarding against this is straightforward: clamp the computed start/end into `[0, text.len()]`, ensure `start <= end`, and short‑circuit (fall back) instead of slicing invalid ranges.

2. **ASCII‑only conversion helpers remain exported, inviting regressions (high)**
   - References: `src/analysis/selection.rs:13-25`, `src/analysis.rs:8-13`
   - `position_to_point` / `point_to_position` intentionally treat UTF‑16 columns as bytes, and the comments warn “only use for ASCII-only contexts”. Despite that, `analysis.rs` still re-exports `position_to_point`, making it trivial for other modules or external consumers to use the buggy helper and reintroduce the multi-byte issues that the rest of the file works hard to avoid. We already have dedicated tests that highlight how this breaks selections when non-ASCII text is present. These helpers should either be removed from the public API, marked `#[deprecated]` with a CompileError, or moved behind a clearly named `*_ascii_only` module so there is no way to accidentally pick them over `PositionMapper`.

3. **Whole selectionRange request fails if any single position is invalid (medium)**
   - References: `src/analysis/selection.rs:1122-1154`, `src/analysis/selection.rs:1186-1219`
   - Both selection range handlers iterate positions and collect them into `Option<Vec<_>>`. This means one `None`—for example when `mapper.position_to_byte` cannot map a cursor because the client sent a stale line/column during a race—causes the entire request to return `None`, dropping results for the other positions. Multi-cursor editors routinely send many locations at once, so a single stale cursor yields no selection ranges anywhere. A more resilient approach is to handle failures per position (skip, or return a zero-length range) and still produce output for the rest, matching LSP expectations of “one result per requested position”.
