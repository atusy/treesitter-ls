# ADR-0019: Lazy Node Identity Tracking

| | |
|---|---|
| **Date** | 2026-01-12 |
| **Status** | Draft |
| **Type** | Core Infrastructure |

**Related ADRs**:
- [ADR-0007](0007-language-server-bridge-virtual-document-model.md) — Virtual document model for injection regions

## Context and Problem Statement

Tree-sitter nodes are ephemeral—they are tied to the lifetime of their parent `Tree` and become invalid after incremental parsing. However, several features require stable node identities across edits:

1. **Bridge virtual documents**: Tracking which injection region a request belongs to
2. **Semantic analysis**: Correlating nodes across multiple LSP operations
3. **Debugging/logging**: Consistent node references in traces

The question is: how do we assign stable identifiers to Tree-sitter nodes that survive incremental parsing?

## Decision Drivers

* **Memory efficiency**: Large documents may have thousands of nodes; tracking all is prohibitive
* **Correctness**: Identifiers must remain valid or be explicitly invalidated after edits
* **Container stability**: Parent nodes (e.g., code blocks) should retain identity when only their contents change
* **Simplicity**: Avoid complex graph diffing algorithms
* **Lifecycle alignment**: Easy cleanup when documents close (`didClose`)

## Considered Options

1. **Eager assignment**: Assign IDs to all nodes on parse
2. **Position-based tracking**: Use `(start_byte, end_byte)` as implicit identity
3. **AST diffing with tree matching**: Match nodes across old/new trees using structural similarity
4. **Lazy assignment with range-overlap invalidation**: Assign IDs on-demand, invalidate any node overlapping the edit range
5. **Lazy assignment with START-priority boundary-based invalidation**: Assign IDs on-demand, invalidate only nodes whose START boundary is inside the edit range

## Decision Outcome

**Chosen option**: "Lazy assignment with START-priority boundary-based invalidation" (Option 5), because it balances memory efficiency (only tracked nodes consume resources), correctness (nodes with changed START boundaries are invalidated), and container stability (parent nodes retain identity when only their contents change).

### Key Behaviors

1. **Lazy assignment**: IDs are assigned only when requested (e.g., `node.id()`)
2. **ULID format**: Universally Unique Lexicographically Sortable Identifiers
3. **START-priority invalidation**: Only nodes whose START position changes (inside the old edit range) are invalidated
4. **Container preservation**: Parent nodes that contain the edit range retain their ID
5. **Per-document storage**: `NodeTracker` lifecycle matches `Document` for easy `didClose` cleanup

### Invalidation Strategy: START-Priority

The key insight is that a node's **identity** is primarily tied to its **START boundary**. The START position defines "where this node begins" and is the primary anchor for identity. The END boundary can shift as contents change.

```
                      edit range
                      |←──────→|
                      ↑        ↑
                   edit.start  edit.old_end

Node A: |←───────────────────────────→|  ✓ KEEP (adjust end)
Node B: |←────────────────→|              ✓ KEEP (end absorbed)
Node C:                |←────────────→|  ✗ INVALIDATE (start inside)
Node D:                  |←──→|          ✗ INVALIDATE (fully inside)
Node E:                          |←────→|  ✓ KEEP (adjust position)
Node F: |←──→|                             ✓ KEEP (unchanged)
```

All "KEEP" cases preserve the node's ULID. Position/size adjustments are applied as needed.

**Rule**: Using the **old tree** coordinates, a node's identity is preserved unless its START boundary **changes**. Invalidate only nodes whose START is inside `[edit.start, edit.old_end)` (i.e., nodes whose START is directly touched by the old edit range). This preserves nodes before **and after** the edit range (e.g., Node E), and avoids invalidating nodes when an edit occurs *at* their START but does not move it.

This matches AST semantics: a `fenced_code_block` remains the "same" block when you edit its contents, because the opening ``` marker (START) defines the block's identity.

## Example: Markdown Code Block

``````markdown
# Title

```python
print("hello")       ← EDIT HERE
```

More text
``````


**Edit**: Change `print("hello")` to `print("hello world")`

| Node | Condition | Result |
|------|-----------|--------|
| `fenced_code_block` | contains edit (START before edit) | ✓ KEEP |
| `code_fence_content` | fully inside edit | ✗ INVALIDATE |
| `paragraph` ("More text") | after edit | ✓ KEEP (position adjusted) |

**The code block's ID is preserved** even though its contents changed.

## Consequences

### Positive

- **Memory efficient**: Only requested nodes are tracked (O(k) where k = tracked nodes)
- **Container stability**: Parent nodes retain ID when contents change
- **Correct invalidation**: Nodes whose START lies inside the old edit range are explicitly removed
- **Clean lifecycle**: `NodeTracker` dies with `Document` on `didClose`
- **AST-aligned semantics**: START-based identity matches structural intuition

### Negative

- **Lookup table rebuild**: After edit, reverse lookup must be rebuilt
- **Nested nodes**: Multiple nodes at same position require `(start, end, kind)` tuple for uniqueness
- **START edits invalidate**: Editing a code block's opening delimiter invalidates its ID
- **START shifts outside range**: Nodes whose START shifts due to earlier edits are preserved

### Neutral

- **ULID choice**: Sortable and unique; could use UUID instead
- **Thread safety**: Requires synchronization (same as `Document` access)
- **Multiple edits**: This architecture extends naturally to batch edits by applying the same logic per edit

## Alternatives Considered

### Option 1: Eager Assignment

Assign IDs to all nodes when the tree is parsed.

* Bad, because **O(n) memory** for all nodes (large documents have 10,000+ nodes)
* Bad, because most IDs are never used

### Option 2: Position-Based Tracking

Use `(start_byte, end_byte)` as implicit identity without explicit IDs.

* Bad, because **no stable identity**—same position after edit could be different node
* Bad, because consumers cannot cache by ID

### Option 3: AST Diffing with Tree Matching

Use structural similarity algorithms to match nodes across old/new trees.

* Bad, because **computationally expensive** (tree matching is O(n²) or worse)
* Bad, because **semantic ambiguity**—what makes two nodes "the same"?

### Option 4: Lazy Assignment with Range-Overlap Invalidation

Invalidate any node whose range overlaps with the edit range.

* Bad, because **invalidates parent nodes**—editing inside a code block invalidates the block itself
* Bad, because **poor for container tracking**—injection regions lose ID on content edit

## Summary

| Aspect | Decision |
|--------|----------|
| **Assignment** | Lazy (on-demand) |
| **Identifier** | ULID |
| **Invalidation** | START-priority boundary-based |
| **Storage** | Per-document |
| **Core rule** | `node.start ∉ [edit.start, edit.old_end)` → identity preserved |
