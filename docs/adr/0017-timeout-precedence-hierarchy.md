# ADR-0017: Timeout Precedence Hierarchy

| | |
|---|---|
| **Date** | 2026-01-06 |
| **Status** | Draft |
| **Type** | Cross-ADR Coordination |

**Related ADRs**:
- [ADR-0013](0013-async-io-layer.md) § Idle Timeout & Initialization Timeout
- [ADR-0015](0015-multi-server-coordination.md) § Response Aggregation
- [ADR-0016](0016-graceful-shutdown.md) § Shutdown Timeout

## Context

The async bridge architecture defines four distinct timeout systems across three ADRs:

1. **Initialization Timeout** (ADR-0013): Bounds server initialization time during startup
2. **Per-Request Timeout** (ADR-0015): Bounds user-facing latency for multi-server aggregation
3. **Idle Timeout** (ADR-0013): Detects hung servers (unresponsive to pending requests)
4. **Global Shutdown Timeout** (ADR-0016): Bounds total shutdown time

### The Problem

These timeout systems have overlapping responsibilities without clear precedence, causing non-deterministic behavior when multiple timeouts could fire simultaneously.

**Conflict Example:**
```
T0: Fan-out completion request to pyright + ruff (5s per-request timeout)
T2: Shutdown initiated (10s global timeout)
T3: pyright responds at T+3s
T4: ruff still pending

Which timeout fires first?
- Per-request timeout: T+5s (from T0)
- Global shutdown timeout: T+12s (from T2)
- Idle timeout: Implementation-defined (30-120s)

What happens to late responses?
```

**Impact Without Precedence:**
- Non-deterministic behavior based on timing
- Timeout interactions undefined (shutdown during per-request timeout?)
- Late response handling ambiguous (accept or discard?)
- Resource cleanup unclear (which timeout triggers state transitions?)

## Decision

**Establish a four-tier timeout hierarchy with explicit precedence rules and interaction semantics.**

### Timeout Tiers

#### Tier 0: Initialization Timeout (Connection Startup)

**Scope**: Per-connection during Initializing state only

**Duration**: Implementation-defined (typically 30-60 seconds)
- Longer than idle timeout (initialization is legitimately slow)

**Trigger**: When `initialize` request sent to downstream server

**Action on Timeout**:
1. Transition connection state: `Initializing` → `Failed`
2. Fail initialization request with `REQUEST_FAILED`
3. Trigger circuit breaker
4. Connection pool schedules retry with backoff

**State Gating**:
- **Enabled**: Only during `Initializing` state
- **Disabled**: When entering `Closing` state (global shutdown takes precedence)

#### Tier 1: Per-Request Timeout (Application Layer)

**Scope**: Single upstream request (may fan out to multiple downstream servers)

**Duration**:
- Explicit requests (hover, definition): 5 seconds
- Incremental requests (completion): 2 seconds

**Trigger**: Only when n ≥ 2 downstream servers participate in aggregation

**Action**:
- Return partial results if at least one server responded
- Return REQUEST_FAILED if all servers timed out

**State Impact**: NONE - Does not affect connection state

#### Tier 2: Idle Timeout (Connection Health)

**Scope**: Per-connection health monitoring

**Duration**: 30-120 seconds (implementation-defined)

**State-Based Gating**:
- **Enabled**: Ready state + pending requests > 0
- **Disabled**: Initializing, Quiescent (no pending), Closing, Failed, Closed

**Action**:
1. Close connection
2. Fail all pending requests with INTERNAL_ERROR
3. Transition connection state: Ready → Failed
4. Trigger circuit breaker
5. Connection pool spawns new instance

#### Tier 3: Global Shutdown Timeout (System Layer)

**Scope**: All connections during shutdown

**Duration**: 5-15 seconds (implementation-defined)

**Trigger**: When shutdown initiated

**Action**:
1. Force kill all remaining server processes (SIGTERM → SIGKILL)
2. Fail all pending operations across all connections
3. Transition all connections: Any state → Closed

**State Impact**: Overrides all other timeouts (highest priority)

### Precedence Rules

**Rule 1: Normal Operation (No Shutdown)**

```
Request sent → Per-request timeout (5s) → Idle timeout (30s)
                        ↓
                Return partial results
                Idle timer: RESET (activity detected)
```

Per-request timeout returns partial results; idle timer resets on stdout activity; connection remains Ready.

**Rule 2: Shutdown Without Pending Requests**

```
Connection state: Ready (quiescent, no pending requests)
Shutdown signal → Idle timeout: DISABLED
              → Per-request timeout: N/A (no requests)
              → Global timeout: ONLY timeout active
```

Idle timeout disabled; only global timeout enforces bounded shutdown time.

**Rule 3: Shutdown With Pending Requests**

```
T0: Request sent (pending: 1)
T1: Shutdown signal → State: Ready → Closing
T2: Idle timeout: DISABLED (state = Closing)
T3: Per-request timeout: Still running
T4: Global timeout: Starts (10s)

Precedence: Global timeout > Per-request timeout
```

Idle timeout **STOPS** when entering Closing state; per-request timeout continues but bounded by global timeout.

**Rule 4: Late Response During Shutdown**

**Decision**: **ACCEPT** late responses until global timeout.

**Rationale**:
- Response provides useful information
- Server is responsive (not hung, just slow)
- Late response resets idle timeout (serves as heartbeat, but idle timeout already stopped in Closing state)

**Rule 5: Shutdown During Initialization**

```
T0: Initialize request sent
    → State: Initializing
    → Init timeout: 60s
    → Idle timeout: DISABLED

T5: Shutdown signal
    → State: Closing
    → Init timeout: CANCELLED
    → Global timeout: 10s STARTS

Active timeouts: Global Shutdown (10s) only
Precedence: Global Shutdown > Initialization
```

Initialization timeout **CANCELLED**; global shutdown timeout takes over; skip LSP shutdown (not initialized), force termination via SIGTERM/SIGKILL.

## Timeout Summary Table

| Scenario | Active Timeouts | Precedence | Final Action |
|----------|----------------|------------|--------------|
| **Initialization** | Initialization only | N/A | Connection → Failed, retry with backoff |
| **Normal operation** | Per-request, Idle | Per-request → Idle resets | Partial results, connection stays Ready |
| **Single-server request** | Idle only | N/A | Connection → Failed on timeout |
| **Shutdown (no pending)** | Global only | N/A | Clean shutdown or force kill |
| **Shutdown (pending)** | Per-request, Global | Global > Per-request | Force kill after global timeout |
| **Shutdown (initializing)** | Global only | Global > Initialization | Skip LSP shutdown, SIGTERM/SIGKILL |
| **Late response in shutdown** | Global only | Accept until global timeout | Deliver if before global timeout |

## Configuration Recommendations

### Timeout Values by Layer

| Timeout Type | Recommended Duration | Rationale |
|-------------|---------------------|-----------|
| **Initialization** | 30-60s | Heavy servers (rust-analyzer) need time to index |
| **Per-Request** | 5s explicit, 2s incremental | User-facing latency bound |
| **Idle** | 30-120s | Detect hung servers without false positives |
| **Global Shutdown** | 5-15s | Total shutdown time, balance clean exit vs user wait |

### Relationships

```
Initialization (60s) > Idle (30-120s) > Per-request (5s)
Global Shutdown (10s) overrides Idle (any duration)

Note: Per-request timeout is NOT part of shutdown sequence
      (requests failed immediately during shutdown per ADR-0016)
```

### Global Shutdown Design: Concurrent Phases

The global timeout is a **single ceiling** for the entire shutdown process. All phases execute **concurrently within** this deadline, not sequentially.

```
Shutdown Timeline (Concurrent):

T0: Start global timeout (e.g., 10s), begin graceful shutdown
    ├─ Writer idle synchronization: ~2s
    ├─ LSP shutdown request/response: ~3-5s
    └─ LSP exit notification + wait for process

T0-T8: Graceful attempts (dynamically determined)
       If successful → Done early (total time: actual, not full timeout)

T8: Graceful timeout → Send SIGTERM (escalation heuristic)
    Reserve ~20% of global timeout for SIGTERM/SIGKILL

T8-T10: Wait for SIGTERM to work (~2s remaining)

T10: Global timeout expires → Send SIGKILL immediately
     Guaranteed termination

Maximum total shutdown time = Global Timeout (exactly)
```

**Constraint**:
```
Global Shutdown ≥ 1s (minimum practical value)

Recommended: 5-15s
  - 5s: Minimum for graceful LSP handshake attempt
  - 10s: Balanced (8s graceful + 2s SIGTERM)
  - 15s: Conservative (12s graceful + 3s SIGTERM)

Escalation heuristic: Reserve 20% of global timeout for SIGTERM
  - Global 10s → Graceful deadline 8s, SIGTERM reserve 2s
```

**Why Concurrent Design**:
- **User experience**: Guaranteed maximum wait time
- **Simplicity**: Single timer, easier to implement and test
- **Parallel efficiency**: Multiple servers shut down concurrently (10s total, not N×10s)
- **Early completion**: If graceful succeeds quickly, total time is actual not timeout

## Consequences

### Positive

**Deterministic Timeout Behavior:**
- Clear precedence when multiple timeouts active
- No timeout interaction races

**Bounded Shutdown Time:**
- Global timeout guarantees system termination
- Predictable maximum latency

**Connection Health Monitoring:**
- Idle timeout detects hung servers during active use
- State-based gating prevents false positives

**User-Facing Latency Bound:**
- Per-request timeout bounds multi-server aggregation
- Partial results provide graceful degradation

### Negative

**Multiple Timeout Concepts:**
- Four distinct timeout systems to understand
- Requires documentation and configuration guidance

**Timeout Value Tuning:**
- Implementation-defined values need careful selection
- Trade-offs between responsiveness and false positives

### Neutral

**LSP Protocol Compliance:**
- Timeouts trigger explicit error responses (not silent hangs)
- Every request receives exactly one response
- Cancellation is best-effort

## Alternatives Considered

### Alternative 1: Single Global Timeout for All Operations

Use one timeout value for initialization, requests, idle, and shutdown.

**Rejected Reasons:**

1. **Conflicting requirements**: Initialization needs long timeout (60s), completion needs short (2s)
2. **Poor UX**: Either too slow for interactive operations or too fast for initialization
3. **No differentiation**: Can't distinguish between hung server and slow operation
4. **Violates separation of concerns**: Different timeout purposes conflated

### Alternative 2: Per-Server Timeout Multiplication

Use per-server timeouts that multiply in multi-server scenarios.

**Rejected Reasons:**

1. **Unbounded total time**: 5 servers × 5s = 25s unacceptable for shutdown
2. **Poor UX**: User waits for slowest server sequentially
3. **No parallel benefit**: Defeats purpose of concurrent shutdown
4. **Complexity**: Must track timeout per server instead of global ceiling

**Why global timeout is better**: Bounded total time (O(1) not O(N)), better UX, simpler implementation.

### Alternative 3: No Timeout Hierarchy (Implicit Precedence)

Let timeout implementation details determine precedence implicitly.

**Rejected Reasons:**

1. **Non-deterministic**: Behavior depends on timing, not explicit rules
2. **Hard to debug**: Unclear which timeout fired in edge cases
3. **No guarantees**: Can't reason about system behavior
4. **Race conditions**: Shutdown vs per-request timeout undefined

**Why explicit hierarchy is better**: Predictable behavior, clear semantics, easier to test.

## Coordination With Other ADRs

### ADR-0013 (Async I/O Layer)

- Idle timeout lifecycle now state-based (Amendment 002)
- Idle timeout STOPS during Closing state
- Reader task cleanup on all timeout paths

### ADR-0014 (Message Ordering)

- Connection state used to compute idle timeout duration
- State transitions triggered by idle timeout (Ready → Failed)
- Pending operations failed when idle timeout fires

### ADR-0015 (Multi-Server)

- Per-request timeout only applies to multi-server aggregation (n≥2)
- Partial results returned when per-request timeout fires
- Circuit breaker triggered by idle timeout (connection-level failure)

### ADR-0016 (Graceful Shutdown)

- Global timeout enforces bounded shutdown time
- Idle timeout disabled when entering Closing state
- Pending operations failed by global timeout if still pending

## Related ADRs

- **[ADR-0013](0013-async-io-layer.md)**: Async I/O layer (defines idle and initialization timeouts)
- **[ADR-0014](0014-actor-based-message-ordering.md)**: Actor-based message ordering (connection state machine)
- **[ADR-0015](0015-multi-server-coordination.md)**: Multi-server coordination (per-request timeout)
- **[ADR-0016](0016-graceful-shutdown.md)**: Graceful shutdown (global shutdown timeout)

## Summary

**Four-Tier Timeout Hierarchy:**

0. **Initialization** (startup): Bounds server initialization time (30-60s)
1. **Per-Request** (application): Bounds user-facing latency for multi-server aggregation (2-5s)
2. **Idle** (connection): Detects hung servers when requests pending (30-120s)
3. **Global Shutdown** (system): Enforces bounded shutdown time across all connections (5-15s)

**Precedence Rules:**
- Global shutdown > Initialization timeout (shutdown during init)
- Global shutdown > Per-request timeout (shutdown during request)
- Global shutdown > Idle timeout (idle timeout STOPS during shutdown)
- Idle timeout DISABLED during Initializing state (initialization timeout applies)
- Late responses accepted until global timeout

**Impact**:
- ✅ Deterministic timeout behavior
- ✅ Clear precedence when multiple timeouts active
- ✅ Bounded shutdown time guaranteed
- ✅ No timeout interaction races
