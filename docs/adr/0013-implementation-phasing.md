# ADR-0013: Implementation Phasing

| | |
|---|---|
| **Date** | 2026-01-08 |
| **Status** | Draft |
| **Type** | Cross-ADR Coordination |

## Context

The async bridge architecture spans multiple ADRs (0014-0018), each defining features at different complexity levels. Without a clear phasing strategy, it's unclear:
- What must be implemented first
- What can be deferred
- How features depend on each other

### Design Principle

**Minimal and Extensible**: Start with the simplest working system, extend incrementally.

## Decision

**Define three implementation phases with clear boundaries and dependencies.**

## Phase Definitions

### Phase 1: Single-LS-per-Language (Current Target)

**Goal**: One downstream language server per language, simple routing, fail-fast error handling.

| Component | Phase 1 Behavior |
|-----------|------------------|
| **Routing** | `languageId` → single server (ADR-0016) |
| **Requests during init** | `REQUEST_FAILED` immediately (ADR-0015) |
| **Cancellation** | Forward `$/cancelRequest` to downstream (ADR-0015) |
| **Timeouts** | Init, Idle, Global Shutdown (ADR-0018) |
| **Coalescing** | None — trust client/server (ADR-0015) |

**What Works:**
- Multiple embedded languages (Python + Lua + TOML in markdown)
- Parallel server initialization
- Per-downstream document lifecycle
- Graceful shutdown with global timeout

**What Doesn't Work Yet:**
- Multiple servers for same language (pyright + ruff for Python)
- Response aggregation/merging
- Circuit breaker / health monitoring

### Phase 2: Resilience Patterns (Future)

**Goal**: Add fault isolation and recovery without changing routing model.

| Component | Phase 2 Addition |
|-----------|------------------|
| **Circuit Breaker** | Track failures, exponential backoff |
| **Health Monitoring** | Per-server health state |
| **Telemetry** | `$/telemetry` events for drops/failures |
| **Coalescing** | Optional, profile-driven (ADR-0015 Future) |

**Why Before Phase 3**: Stabilize single-server-per-language before adding aggregation complexity.

**Dependencies**: Phase 1 complete.

### Phase 3: Multi-LS-per-Language (Future)

**Goal**: Multiple servers for the same language with response aggregation.

| Component | Phase 3 Addition |
|-----------|------------------|
| **Routing** | Fan-out to multiple servers (ADR-0016) |
| **Aggregation** | merge_all, first_wins, ranked strategies |
| **Per-Request Timeout** | Bounds aggregation latency (ADR-0018) |
| **Backpressure** | Multi-server coordination (ADR-0016) |

**Use Case**: pyright (types) + ruff (linting) for Python simultaneously.

**Dependencies**: Phase 1 complete. Phase 2 recommended for resilience.

## Phase Feature Matrix

| Feature | Phase 1 | Phase 2 | Phase 3 |
|---------|:-------:|:-------:|:-------:|
| Multiple languages | ✅ | ✅ | ✅ |
| Simple routing (lang→server) | ✅ | ✅ | ✅ |
| Parallel initialization | ✅ | ✅ | ✅ |
| Graceful shutdown | ✅ | ✅ | ✅ |
| Fail-fast during init | ✅ | ✅ | ✅ |
| Cancellation forwarding | ✅ | ✅ | ✅ |
| Circuit breaker | ❌ | ✅ | ✅ |
| Health monitoring | ❌ | ✅ | ✅ |
| Multiple servers/language | ❌ | ❌ | ✅ |
| Response aggregation | ❌ | ❌ | ✅ |
| Per-request timeout | ❌ | ❌ | ✅ |

## ADR Phase Mapping

| ADR | Phase 1 Scope | Phase 2 Additions | Phase 3 Additions |
|-----|---------------|-------------------|-------------------|
| **0014** (Connection) | Async I/O, timeouts | Circuit breaker hooks | — |
| **0015** (Ordering) | Thin bridge, forwarding | Optional coalescing | — |
| **0016** (Coordination) | Simple routing, lifecycle | Health monitoring | Aggregation, fan-out |
| **0017** (Shutdown) | Graceful shutdown | — | — |
| **0018** (Timeouts) | Init, Idle, Global | — | Per-request timeout |

## Consequences

### Positive

- **Clear implementation order**: Know what to build first
- **Reduced complexity**: Each ADR can focus on current phase
- **Testable milestones**: Phase 1 is a complete, working system

### Negative

- **Feature limitations**: Phase 1 can't do pyright + ruff simultaneously
- **Documentation overhead**: Must track which phase each feature belongs to

### Neutral

- **Extensibility preserved**: Phase 2/3 features documented but not blocking

## Related ADRs

- **[ADR-0014](0014-async-bridge-connection.md)**: Async Bridge Connection
- **[ADR-0015](0015-message-ordering.md)**: Message Ordering
- **[ADR-0016](0016-server-pool-coordination.md)**: Server Pool Coordination
- **[ADR-0017](0017-graceful-shutdown.md)**: Graceful Shutdown
- **[ADR-0018](0018-timeout-hierarchy.md)**: Timeout Hierarchy
