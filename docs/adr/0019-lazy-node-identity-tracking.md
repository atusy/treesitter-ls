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
* **Performance**: Minimal overhead for incremental parsing

## Considered Options

1. **Eager assignment**: Assign IDs to all nodes on parse
2. **Position-based tracking**: Use `(start_byte, end_byte)` as implicit identity
3. **AST diffing with tree matching**: Match nodes across old/new trees using structural similarity
4. **Lazy assignment with range-overlap invalidation**: Assign IDs on-demand, invalidate any node overlapping the edit range
5. **Lazy assignment with boundary-based invalidation**: Assign IDs on-demand, invalidate only nodes whose boundaries are affected

## Decision Outcome

**Chosen option**: "Lazy assignment with boundary-based invalidation" (Option 5), because it balances memory efficiency (only tracked nodes consume resources), correctness (nodes with changed boundaries are invalidated), and container stability (parent nodes retain identity when only their contents change).

### Core Design

```rust
struct NodeTracker {
    /// ULID → node metadata
    tracked: HashMap<Ulid, TrackedNodeInfo>,
    /// Reverse lookup: (start_byte, end_byte, node_kind) → ULID
    lookup: HashMap<(usize, usize, &'static str), Ulid>,
}

struct TrackedNodeInfo {
    start_byte: usize,
    end_byte: usize,
    node_kind: &'static str,
}
```

### Key Behaviors

1. **Lazy assignment**: IDs are assigned only when `node.id()` is called
2. **ULID format**: Universally Unique Lexicographically Sortable Identifiers
3. **Boundary-based invalidation**: Only nodes whose start/end positions fall within the edit range are invalidated
4. **Container preservation**: Parent nodes that contain the edit range retain their ID (with adjusted size)

### Invalidation Strategy: START-Priority Boundary-Based

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

**Rule**: If a node's START is outside (before) the edit range, its identity is preserved.

This matches AST semantics: a `fenced_code_block` remains the "same" block when you edit its contents, because the opening ``` marker (START) defines the block's identity. The closing marker (END) can move without changing identity.

### Edit Algorithm

```rust
fn on_edit(&mut self, edit: &InputEdit) {
    let delta = edit.new_end_byte as isize - edit.old_end_byte as isize;

    self.tracked.retain(|_id, info| {
        let node_start = info.start_byte;
        let node_end = info.end_byte;
        let edit_start = edit.start_byte;
        let edit_old_end = edit.old_end_byte;

        // Case 1 (Node A): Node fully contains edit range → KEEP, adjust end
        if node_start < edit_start && node_end > edit_old_end {
            info.end_byte = (node_end as isize + delta) as usize;
            return true;
        }

        // Case 2 (Node B): Node starts before edit, ends inside edit → KEEP, absorb edit
        // START is preserved (identity anchor), END absorbs the edit endpoint
        if node_start < edit_start && node_end > edit_start && node_end <= edit_old_end {
            info.end_byte = edit.new_end_byte;
            return true;
        }

        // Case 3 (Node F): Node is entirely before edit → KEEP unchanged
        if node_end <= edit_start {
            return true;
        }

        // Case 4 (Node E): Node is entirely after edit → KEEP, adjust position
        if node_start >= edit_old_end {
            info.start_byte = (node_start as isize + delta) as usize;
            info.end_byte = (node_end as isize + delta) as usize;
            return true;
        }

        // Case 5 (Node C, D): Node starts inside edit range → INVALIDATE
        // START boundary was modified, identity is lost
        false
    });

    // Rebuild lookup table with updated positions
    self.rebuild_lookup();
}
```

### Storage Location

**Per-document storage**: `NodeTracker` is stored within each `Document` struct.

```rust
pub struct Document {
    pub content: String,
    pub tree: Tree,
    pub node_tracker: NodeTracker,  // New field
}
```

**Rationale**:
- `didClose` cleanup is trivial (`documents.remove(&uri)` drops tracker)
- No URI overhead in lookup keys
- Lifecycle matches document lifetime
- Simpler concurrent access (per-document locking)

## Example: Markdown Code Block

```markdown
# Title              ← bytes 0-8

```python            ← bytes 10-20 (fenced_code_block start boundary)
print("hello")       ← bytes 20-35 ← EDIT HERE
```                  ← bytes 35-38 (fenced_code_block end boundary)

More text            ← bytes 40-50
```

**Edit**: Change `print("hello")` to `print("hello world")` (bytes 20-35 → 20-45)

| Node | Range | Condition | Result |
|------|-------|-----------|--------|
| `document` | 0-50 | contains edit | ✓ KEEP (end: 50→60) |
| `fenced_code_block` | 10-38 | contains edit | ✓ KEEP (end: 38→48) |
| `code_fence_content` | 20-35 | fully inside edit | ✗ INVALIDATE |
| `paragraph` | 40-50 | after edit | ✓ ADJUST (50→60) |

**The code block's ID is preserved** even though its contents changed.

## Consequences

### Positive

- **Memory efficient**: Only requested nodes are tracked
- **Container stability**: Parent nodes (code blocks, functions) retain ID when contents change
- **Correct invalidation**: Nodes with changed boundaries are explicitly removed
- **Simple algorithm**: Position adjustment is O(n) where n = tracked nodes
- **Clean lifecycle**: NodeTracker dies with Document on `didClose`
- **AST-aligned semantics**: Boundary-based identity matches structural intuition

### Negative

- **Lookup table rebuild**: After edit, `lookup` HashMap must be rebuilt
- **Nested nodes**: Multiple nodes at same position require `(start, end, kind)` tuple for uniqueness
- **START edits invalidate**: Editing a code block's opening ``` invalidates its ID (by design)

### Neutral

- **ULID choice**: Sortable and unique; no particular drawback vs UUID
- **Thread safety**: Requires synchronization when accessing `NodeTracker` (already needed for `Document`)

## Alternatives Considered in Detail

### Option 1: Eager Assignment

Assign IDs to all nodes when the tree is parsed.

* Good, because every node has an ID immediately available
* Bad, because **O(n) memory** for all nodes (large documents have 10,000+ nodes)
* Bad, because **O(n) rebuild** on every edit
* Bad, because most IDs are never used

### Option 2: Position-Based Tracking

Use `(start_byte, end_byte)` as implicit identity without explicit IDs.

* Good, because zero additional storage
* Good, because naturally invalidates on position change
* Bad, because **no stable identity**—same position after edit could be different node
* Bad, because nested nodes share start position
* Bad, because consumers cannot cache by ID

### Option 3: AST Diffing with Tree Matching

Use structural similarity algorithms to match nodes across old/new trees.

* Good, because preserves identity through content edits
* Bad, because **computationally expensive** (tree matching is O(n²) or worse)
* Bad, because **semantic ambiguity**—what makes two nodes "the same"?
* Bad, because complex implementation with many edge cases

### Option 4: Lazy Assignment with Range-Overlap Invalidation

Invalidate any node whose range overlaps with the edit range.

* Good, because **O(k) memory** where k = explicitly tracked nodes
* Good, because **simple implementation**
* Bad, because **invalidates parent nodes**—editing inside a code block invalidates the block itself
* Bad, because **poor for container tracking**—injection regions lose ID on content edit

### Option 5: Lazy Assignment with START-Priority Boundary-Based Invalidation (Chosen)

* Good, because **O(k) memory** where k = explicitly tracked nodes
* Good, because **O(k) edit handling**—only tracked nodes processed
* Good, because **container stability**—parent nodes retain ID when contents change
* Good, because **START-priority**—identity anchored to where node begins, matching Tree-sitter semantics
* Good, because **predictable semantics**—clear rule: START outside edit → KEEP
* Neutral, because 5 cases in algorithm (vs 2 for range-overlap)

## Implementation Notes

### Handling Nested Nodes

Multiple nodes can share the same `start_byte` (e.g., `function_definition` contains `identifier`). The lookup key uses a 3-tuple:

```rust
type LookupKey = (usize, usize, &'static str);  // (start, end, kind)
```

This ensures uniqueness as long as no two nodes of the same kind have identical spans (which Tree-sitter guarantees).

### Thread Safety

The current `DocumentStore` uses `DashMap<Url, Document>`. NodeTracker access follows the same pattern:

```rust
// Safe: entire Document is accessed atomically
if let Some(doc) = self.documents.get(&uri) {
    let id = doc.node_tracker.get_or_assign_id(&node);
}
```

### ULID Generation

```rust
use ulid::Ulid;

fn get_or_assign_id(&mut self, node: &Node) -> Ulid {
    let key = (node.start_byte(), node.end_byte(), node.kind());
    *self.lookup.entry(key).or_insert_with(|| {
        let id = Ulid::new();
        self.tracked.insert(id, TrackedNodeInfo { /* ... */ });
        id
    })
}
```

### Edge Cases

| Scenario | Behavior |
|----------|----------|
| Edit inside node (content only) | Node A: KEEP, end adjusted |
| Edit extends past node's end | Node B: KEEP, end absorbs edit endpoint |
| Edit at node START (e.g., add char at start) | Node C: INVALIDATE (start inside edit) |
| Insert before node | Node E: KEEP, position adjusted |
| Delete entire node | Node D: INVALIDATE (fully inside edit) |
| Edit closing delimiter only | KEEP (END can move, START is identity) |
| Edit opening delimiter | INVALIDATE (START is identity anchor) |

## Summary

**Design**: Lazy ULID assignment with position-based lookup and START-priority boundary-based invalidation

**Key insight**: A node's identity is anchored to its START boundary; END can shift without losing identity

**Core rule**: `node.start_byte < edit.start_byte` → identity preserved

**Primary use case**: Stable injection region tracking for LSP bridge virtual documents

**Integration**: Per-document storage in `Document` struct for lifecycle alignment
