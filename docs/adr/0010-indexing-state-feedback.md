# ADR-0010: Indexing State Feedback for Bridged LSP Features

| | |
|---|---|
| **Status** | accepted |
| **Date** | 2026-01-02 |
| **Decision-makers** | atusy |
| **Consulted** | - |
| **Informed** | - |

## Context and Problem Statement

Language servers like rust-analyzer require an **indexing phase** after startup before they can provide accurate results. During this phase:

- Type information is being collected
- Symbol tables are being built
- Cross-reference data is incomplete

When a user triggers hover (or other LSP features) on a Markdown code block immediately after the server spawns, the bridge may receive empty or incomplete results because rust-analyzer is still indexing.

```
┌─────────────────────────────────────────────────────────────────┐
│ Timeline                                                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  [Server Spawn]──────[Indexing...]──────[Ready]                 │
│        │                   │                │                   │
│        │     User hovers   │                │                   │
│        │         │         │                │                   │
│        │         ▼         │                │                   │
│        │    ❌ Empty/null  │                │                   │
│        │      response     │                │                   │
│        │                   │                │                   │
│        │                   │     User hovers again              │
│        │                   │          │                         │
│        │                   │          ▼                         │
│        │                   │     ✅ Full response               │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

The question is: how should the bridge handle requests that arrive while the language server is still indexing?

## Decision Drivers

* **User experience**: Users should understand why a feature isn't working
* **Transparency**: Server state should be visible, not hidden
* **Simplicity**: Avoid complex $/progress notification monitoring
* **Non-blocking**: Don't delay connection availability waiting for indexing
* **Consistency**: Same behavior across different language servers

## Considered Options

1. **Block until indexing completes** — Wait for $/progress `rustAnalyzer/indexing` end notification
2. **Return informative message** — Return "indexing (rust-analyzer)" instead of empty result
3. **Silent empty response** — Return null/empty, let user figure it out
4. **Retry loop with backoff** — Automatically retry until result or timeout

## Decision Outcome

**Chosen option**: "Return informative message" (Option 2), because it provides immediate feedback, requires no complex state tracking, and follows the principle of transparency over magic.

### Implementation Per LSP Feature

| Feature | During Indexing Response | Notes |
|---------|-------------------------|-------|
| hover | `{ contents: "⏳ indexing (rust-analyzer)" }` | Clear, actionable |
| completion | Empty list `[]` | Can't show message in completion UI |
| signatureHelp | `null` | Same as "not applicable" |
| definition | `null` | Same as "no definition found" |
| references | `[]` | Same as "no references" |

**Hover is special**: It's the only feature where we can meaningfully show a status message because its response is displayed directly to the user in a popup.

### Detecting Indexing State

```rust
enum ServerState {
    Indexing,   // After spawn, before first successful response
    Ready,      // After first non-empty response received
}
```

**Heuristic approach** (simpler than $/progress monitoring):
- Start in `Indexing` state after spawn
- Transition to `Ready` after receiving any non-empty hover/completion response
- This works because rust-analyzer returns empty results during indexing

### Consequences

**Positive:**
* Users immediately understand the situation ("Oh, it's indexing, I'll wait")
* No complex $/progress notification monitoring required
* Connection available immediately (no blocking)
* Consistent with VS Code and other editors that show "Loading..." states
* Simple state machine (2 states vs. tracking arbitrary progress tokens)

**Negative:**
* User must manually retry (no automatic refresh)
* Can't distinguish "indexing" from "server error" in some cases
* Heuristic may misclassify edge cases (empty file = legitimately empty response)

**Neutral:**
* Other features (completion, definition) still return empty during indexing
* Future enhancement could add auto-refresh when ready

### Confirmation

* E2E test: Trigger hover immediately after server spawn → verify "indexing" message returned
* E2E test: Wait for indexing, trigger hover → verify normal hover content returned
* E2E test: Open small code block → verify transition from "indexing" to "ready" state

## Pros and Cons of the Options

### Option 1: Block until indexing completes

Wait for `$/progress` notification with `rustAnalyzer/indexing` token and `kind: 'end'`.

* Good, because first request always succeeds
* Good, because no user action required
* Bad, because **adds significant complexity** (notification monitoring, token tracking)
* Bad, because **blocks connection availability** for potentially 60+ seconds on large projects
* Bad, because **server-specific** (rust-analyzer uses specific token; other servers differ)
* Bad, because user has no visibility into why things are slow

### Option 2: Return informative message

Return "indexing (rust-analyzer)" in hover response when server appears to be indexing.

* Good, because **immediate feedback** — user knows what's happening
* Good, because **simple implementation** — state heuristic, no notification monitoring
* Good, because **non-blocking** — connection available immediately
* Good, because **transparent** — follows "explicit > implicit" principle
* Good, because **consistent with industry** — VS Code shows "Loading..." similarly
* Neutral, because user must retry manually
* Bad, because heuristic detection may have edge cases

### Option 3: Silent empty response

Return null/empty response, same as "not found".

* Good, because simplest implementation (no change needed)
* Bad, because **poor UX** — user has no idea why hover isn't working
* Bad, because **indistinguishable** from "no information available"
* Bad, because leads to user confusion and bug reports

### Option 4: Retry loop with backoff

Automatically retry the request every N seconds until result or timeout.

* Good, because eventually succeeds without user action
* Bad, because **unpredictable latency** — first hover could take 30+ seconds
* Bad, because **wastes resources** — repeated requests during indexing
* Bad, because **hides the problem** — user doesn't know retry is happening
* Bad, because complex implementation (background tasks, cancellation)

## More Information

### Related ADRs

* [ADR-0006](0006-language-server-bridge.md): Core LSP bridge architecture
* [ADR-0008](0008-language-server-bridge-request-strategies.md): Per-method bridge strategies
* [ADR-0009](0009-async-bridge-architecture.md): Async architecture for concurrent requests

### rust-analyzer Indexing Behavior

rust-analyzer sends `$/progress` notifications during indexing:

```json
// Begin
{"jsonrpc":"2.0","method":"$/progress","params":{"token":"rustAnalyzer/indexing","value":{"kind":"begin","title":"Indexing","percentage":0}}}

// Progress updates
{"jsonrpc":"2.0","method":"$/progress","params":{"token":"rustAnalyzer/indexing","value":{"kind":"report","percentage":50}}}

// End
{"jsonrpc":"2.0","method":"$/progress","params":{"token":"rustAnalyzer/indexing","value":{"kind":"end"}}}
```

While we *could* monitor these, the heuristic approach (detect via empty responses) is simpler and works across different language servers.

### Future Enhancements

1. **Auto-refresh**: When transitioning from `Indexing` to `Ready`, could trigger UI refresh
2. **Progress indication**: Could show indexing percentage if $/progress is monitored
3. **Per-language messages**: "indexing (rust-analyzer)" vs "indexing (lua-language-server)"
