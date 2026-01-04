// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Rustacean editing Markdown",
  "developer editing Lua files",
  "documentation author with Rust code blocks",
  "treesitter-ls user managing configurations",
] as const satisfies readonly string[]; // Must have at least one role. Avoid generic roles like "user" or "admin". Remove obsolete roles freely.

const scrum: ScrumDashboard = {
  product_goal: {
    statement: "Expand LSP bridge to support most language server features indirectly through bridging (ADR-0006, 0007, 0008)",
    success_metrics: [
      { metric: "Bridge coverage", target: "Support completion, signatureHelp, references, rename, codeAction, formatting, typeDefinition, implementation, documentHighlight, declaration, inlayHint, callHierarchy, typeHierarchy, documentLink, foldingRange" },
      { metric: "Modular architecture", target: "Bridge module organized with text_document/ subdirectory matching lsp_impl structure" },
      { metric: "E2E test coverage", target: "Each bridged feature has E2E test verifying end-to-end flow" },
    ],
  },

  // Completed PBIs: PBI-001 through PBI-140 (Sprint 1-113), PBI-155-161 (Sprint 124-130), PBI-178-180a (Sprint 133-135), PBI-184 (Sprint 136)
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  // Removed: PBI-163-177 (obsolete - created before greenfield deletion per ASYNC_BRIDGE_REMOVAL.md)
  // Superseded: PBI-183 (merged into PBI-180b during Sprint 136 refinement)
  product_backlog: [
    // ADR-0012 Phase 1: Single-LS-per-Language Foundation (PBI-178-184 done, Sprint 133-136)
    // Sprint 137 candidates: PBI-181 hover (ready, highest priority), PBI-180b superseding (ready)
    // Future: PBI-182 definition/signatureHelp (draft - needs AC refinement)
    {
      id: "PBI-181",
      story: {
        role: "developer editing Lua files",
        capability: "see hover information for Lua code in markdown code blocks",
        benefit: "I can understand Lua APIs and types without leaving the markdown document",
      },
      acceptance_criteria: [
        { criterion: "Pool.hover() method calls BridgeConnection.send_request with textDocument/hover", verification: "grep 'send_request.*hover\\|hover.*send_request' src/lsp/bridge/pool.rs" },
        { criterion: "Hover request uses virtual document URI and translated position from host coordinates", verification: "grep -E 'virtual.*uri|translate.*position' src/lsp/bridge/pool.rs" },
        { criterion: "Hover response (Hover with contents) returned to host without range translation (hover ranges are optional)", verification: "grep -E 'pool.hover|Hover' src/lsp/lsp_impl/text_document/hover.rs" },
        { criterion: "E2E test using treesitter-ls binary receives real hover information from lua-ls for Lua built-in (e.g., print)", verification: "cargo test --test e2e_lsp_lua_hover --features e2e" },
      ],
      status: "ready" as PBIStatus,
      refinement_notes: [
        "SPRINT 137 REFINEMENT: Confirmed ready for sprint - all ACs testable with correct E2E pattern",
        "SPRINT 137 REFINEMENT: Updated AC4 E2E verification to use binary-first pattern (LspClient, not BridgeConnection)",
        "ALIGNED WITH SPRINT 136 LEARNING: E2E test must spawn treesitter-ls binary, following e2e_lsp_lua_completion.rs pattern",
        "SIMPLIFIED: 4 ACs (hover ranges optional in LSP spec, superseding deferred to PBI-180b)",
        "UNBLOCKED: PBI-184 done (Sprint 136) - pool now spawns/manages BridgeConnection per language",
        "COMPLEXITY: Low-Medium - directly reuses send_request infrastructure from Sprint 135",
        "PRIORITY: Highest in ADR-0012 Phase 1 backlog - direct user value for API documentation",
      ],
    },
    {
      id: "PBI-180b",
      story: {
        role: "developer editing Lua files",
        capability: "have stale incremental requests cancelled when typing rapidly",
        benefit: "I only see relevant suggestions for current code, not outdated results from earlier positions",
      },
      acceptance_criteria: [
        { criterion: "PendingIncrementalRequests struct tracks latest completion/hover/signatureHelp per connection", verification: "grep 'struct PendingIncrementalRequests' src/lsp/bridge/connection.rs" },
        { criterion: "Request superseding: newer incremental request cancels older with REQUEST_FAILED and superseded reason", verification: "grep -E 'register_completion|register_hover|REQUEST_FAILED.*superseded' src/lsp/bridge/connection.rs" },
        { criterion: "Phase 2 guard: requests wait for initialized with bounded timeout (5s default)", verification: "grep -B5 -A5 'wait_for_initialized' src/lsp/bridge/connection.rs" },
        { criterion: "Phase 2 guard: document notifications (didChange, didSave) dropped before didOpen sent", verification: "grep -B5 -A5 'did_open_sent' src/lsp/bridge/connection.rs | grep 'didChange\\|didSave'" },
        { criterion: "E2E test verifies rapid completion requests trigger superseding with only latest processed", verification: "cargo test --test e2e_bridge_init_race --features e2e" },
        { criterion: "All unit tests pass with superseding infrastructure", verification: "make test" },
      ],
      status: "ready" as PBIStatus,
      refinement_notes: [
        "SPRINT 137 REFINEMENT: Promoted to ready - all dependencies met, ACs clear",
        "SPRINT 137 REFINEMENT: 6 ACs at upper complexity threshold but keeping unified (user value: responsive UX)",
        "SPRINT 136 REFINEMENT: MERGED WITH PBI-183 to eliminate duplication",
        "SCOPE: General request superseding infrastructure for all incremental requests (completion, hover, signatureHelp)",
        "SCOPE: Phase 2 guard implementation (wait pattern + document notification dropping)",
        "DEPENDENCY: Requires PBI-180a infrastructure (send_request, request ID tracking, timeout) ✓ DONE Sprint 135",
        "CONSOLIDATION: Combined PBI-180b (Phase 2 guard) + PBI-183 (general superseding) into single infrastructure PBI",
        "RATIONALE: Both PBIs had identical user stories; PBI-183 was the infrastructure layer that PBI-180b depended on",
        "VALUE: Prevents stale results during rapid typing (initialization window) and normal operation",
        "COMPLEXITY: Medium-High - introduces new patterns (PendingIncrementalRequests, Phase 2 guard)",
        "NEXT STEPS: Review ADR-0012 Phase 1 §6.1 (Phase 2 guard) and §7.3 (request superseding) for implementation guidance",
      ],
    },
    {
      id: "PBI-182",
      story: {
        role: "developer editing Lua files",
        capability: "navigate to definitions in Lua code blocks and see signature help",
        benefit: "I can explore Lua code structure and write correct function calls",
      },
      acceptance_criteria: [
        { criterion: "BridgeConnection handles textDocument/definition requests with position translation", verification: "grep 'textDocument/definition' src/lsp/bridge/connection.rs" },
        { criterion: "BridgeConnection handles textDocument/signatureHelp requests", verification: "grep 'textDocument/signatureHelp' src/lsp/bridge/connection.rs" },
        { criterion: "E2E test verifies goto definition returns real locations from lua-ls", verification: "cargo test --test e2e_bridge_definition --features e2e" },
        { criterion: "E2E test verifies signature help shows real function signatures", verification: "cargo test --test e2e_bridge_signature --features e2e" },
      ],
      status: "draft" as PBIStatus,
      refinement_notes: [
        "SPRINT 137 REFINEMENT: Kept as draft - ACs need correction before ready",
        "ISSUE: AC1-AC2 reference BridgeConnection layer instead of Pool methods (wrong layer per architecture)",
        "ISSUE: AC3-AC4 use e2e_bridge_* naming (wrong pattern per docs/e2e-testing-checklist.md)",
        "NEEDED: Rewrite ACs to specify Pool.definition() and Pool.signature_help() methods",
        "NEEDED: Update E2E test names to e2e_lsp_lua_definition.rs and e2e_lsp_lua_signature.rs (binary-first pattern)",
        "CONSIDERATION: May split into PBI-182a (definition) and PBI-182b (signatureHelp) - two distinct features",
        "DEPENDENCY: Infrastructure exists (PBI-180a, PBI-184) - technical readiness confirmed",
        "NEXT STEPS: Schedule dedicated refinement session to rewrite ACs with correct layer/pattern",
      ],
    },
    // PBI-183: SUPERSEDED BY PBI-180b (merged during Sprint 136 refinement)
    // Rationale: PBI-183 and PBI-180b had identical user stories and overlapping ACs
    // PBI-180b now covers both general superseding infrastructure (from PBI-183) and Phase 2 guard
    // See PBI-180b refinement_notes for consolidation details
    // Future: Phase 2 (circuit breaker, bulkhead, health monitoring), Phase 3 (multi-LS routing, aggregation)
  ],
  sprint: null,
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
      { name: "Documentation updated alongside implementation", run: "git diff --name-only | grep -E '(README|docs/|adr/)' || echo 'No docs updated - verify if needed'" },
      { name: "ADR verification for architectural changes", run: "git diff --name-only | grep -E 'adr/' || echo 'No ADR updated - verify if architectural change'" },
    ],
  },
  // Historical sprints (recent 3) | Sprint 1-133: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 136, pbi_id: "PBI-184", goal: "Wire bridge infrastructure to treesitter-ls binary with connection lifecycle management", status: "done", subtasks: [
      { test: "DashMap<String, Arc<BridgeConnection>> for per-language connections", implementation: "Replace _connection: Option with DashMap for concurrent access", type: "behavioral", status: "completed", commits: [{ hash: "1c61df2", message: "feat(bridge): add DashMap for per-language connections", phase: "green" }], notes: [] },
      { test: "get_or_spawn_connection(language) spawns on first access", implementation: "DashMap entry API with lazy initialization", type: "behavioral", status: "completed", commits: [{ hash: "df04faf", message: "feat(bridge): implement lazy connection spawning", phase: "green" }], notes: [] },
      { test: "Pool.completion() uses real send_request", implementation: "Extract language from URI, spawn connection, forward request", type: "behavioral", status: "completed", commits: [{ hash: "d3fbae8", message: "feat(bridge): wire completion to real send_request", phase: "green" }], notes: [] },
      { test: "E2E test via LspClient (treesitter-ls binary)", implementation: "tests/e2e_lsp_lua_completion.rs with correct E2E pattern", type: "behavioral", status: "completed", commits: [{ hash: "797ad7a", message: "feat(bridge): add proper E2E test via LspClient", phase: "green" }], notes: ["lua-ls returns null until didOpen with virtual content implemented"] },
      { test: "Deprecate wrong-layer e2e_bridge tests", implementation: "Add deprecation comments pointing to correct pattern", type: "structural", status: "completed", commits: [{ hash: "5c26c45", message: "docs: deprecate wrong-layer E2E tests", phase: "green" }], notes: [] },
    ] },
    { number: 135, pbi_id: "PBI-180a", goal: "Real LSP completion with request/response infrastructure", status: "done", subtasks: [
      { test: "Request ID tracking and correlation", implementation: "AtomicU64 + synchronous read loop", type: "behavioral", status: "completed", commits: [{ hash: "6073632", message: "feat(bridge): implement send_request", phase: "green" }], notes: [] },
      { test: "Position translation host→virtual", implementation: "CacheableInjectionRegion.translate_host_to_virtual()", type: "behavioral", status: "completed", commits: [{ hash: "eaac2b8", message: "feat(bridge): position translation", phase: "green" }], notes: [] },
      { test: "5s timeout with REQUEST_FAILED", implementation: "tokio::time::timeout", type: "behavioral", status: "completed", commits: [{ hash: "6073632", message: "feat(bridge): implement send_request", phase: "green" }], notes: [] },
    ] },
    { number: 134, pbi_id: "PBI-179", goal: "Real LSP initialization with lua-language-server", status: "done", subtasks: [
      { test: "Spawn lua-ls process", implementation: "tokio::process::Command", type: "behavioral", status: "completed", commits: [{ hash: "ddc4875", message: "feat(bridge): spawn process", phase: "green" }], notes: [] },
      { test: "JSON-RPC framing", implementation: "Content-Length headers", type: "behavioral", status: "completed", commits: [{ hash: "707496d", message: "feat(bridge): JSON-RPC framing", phase: "green" }], notes: [] },
      { test: "Initialize handshake + Phase 1 guard", implementation: "initialize→initialized→didOpen + SERVER_NOT_INITIALIZED", type: "behavioral", status: "completed", commits: [{ hash: "2a5300e", message: "feat(bridge): initialize protocol", phase: "green" }], notes: [] },
    ] },
  ],
  // Retrospectives (recent 3) | Sprints 1-133: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    { sprint: 136, improvements: [
      { action: "Create E2E testing anti-pattern checklist (test through binary not library, must use LspClient, verification criteria for E2E tests)", timing: "immediate", status: "completed", outcome: "Created docs/e2e-testing-checklist.md documenting binary-first principle and verification criteria from Sprint 136 experience" },
      { action: "Assess deprecated e2e_bridge tests for removal (e2e_bridge_completion.rs tests wrong layer, e2e_bridge_fakeit.rs tests obsolete phase)", timing: "immediate", status: "completed", outcome: "Kept deprecated tests with clear deprecation comments - provide historical documentation value; e2e_lsp_lua_completion.rs is now canonical E2E pattern" },
      { action: "Add 'wire early' principle to ADR-0012 (wire infrastructure incrementally vs batch at end, reduces feedback delay)", timing: "sprint", status: "active", outcome: null },
    ] },
    { sprint: 135, improvements: [
      { action: "Create deferred work tracking checklist (issue templates, AC marking convention, follow-up PBI creation criteria)", timing: "immediate", status: "completed", outcome: "Created docs/deferred-work-tracking-checklist.md with marking conventions and follow-up criteria from Sprint 135 experience" },
      { action: "Add Pool-to-BridgeConnection integration subtask to next sprint to complete range translation deferred from PBI-180a AC3", timing: "sprint", status: "active", outcome: null },
      { action: "Document PBI splitting criteria in ADR-0012 (complexity threshold: 5-6 ACs triggers split consideration, dependency extraction patterns)", timing: "sprint", status: "active", outcome: null },
      { action: "Add async boundary placement guidance to ADR-0012 Phase 2 (minimize ripple effects, prefer boundaries at module edges)", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 134, improvements: [
      { action: "Document E2E testing patterns (feature-gated visibility, testability vs API surface trade-offs) in ADR-0012 or new ADR-0013", timing: "sprint", status: "active", outcome: null },
      { action: "Create JSON-RPC framing checklist (Content-Length, \\r\\n\\r\\n separator, UTF-8 byte length, async I/O patterns)", timing: "immediate", status: "completed", outcome: "Created docs/json-rpc-framing-checklist.md with implementation guidance from Sprint 134" },
      { action: "Add performance budgets to ADR-0012 Phase 2/3 (init < 200ms, completion < 100ms, superseding < 10ms)", timing: "product", status: "active", outcome: null },
    ] },
  ],
};

// Type Definitions (DO NOT MODIFY) =============================================
// PBI lifecycle: draft (idea) -> refining (gathering info) -> ready (can start) -> done
type PBIStatus = "draft" | "refining" | "ready" | "done";

// Sprint lifecycle
type SprintStatus = "planning" | "in_progress" | "review" | "done" | "cancelled";

// TDD cycle: pending -> red (test written) -> green (impl done) -> refactoring -> completed
type SubtaskStatus = "pending" | "red" | "green" | "refactoring" | "completed";

// behavioral = changes observable behavior, structural = refactoring only
type SubtaskType = "behavioral" | "structural";

// Commits happen only after tests pass (green/refactoring), never on red
type CommitPhase = "green" | "refactoring";

// When to execute retrospective actions:
//   immediate: Apply within Retrospective (non-production code, single logical change)
//   sprint: Add as subtask to next sprint (process improvements)
//   product: Add as new PBI to Product Backlog (feature additions)
type ImprovementTiming = "immediate" | "sprint" | "product";

type ImprovementStatus = "active" | "completed" | "abandoned";

interface SuccessMetric { metric: string; target: string; }
interface ProductGoal { statement: string; success_metrics: SuccessMetric[]; }
interface AcceptanceCriterion { criterion: string; verification: string; }
interface UserStory { role: (typeof userStoryRoles)[number]; capability: string; benefit: string; }
interface PBI { id: string; story: UserStory; acceptance_criteria: AcceptanceCriterion[]; status: PBIStatus; refinement_notes?: string[]; }
interface Commit { hash: string; message: string; phase: CommitPhase; }
interface Subtask { test: string; implementation: string; type: SubtaskType; status: SubtaskStatus; commits: Commit[]; notes: string[]; }
interface Sprint { number: number; pbi_id: string; goal: string; status: SprintStatus; subtasks: Subtask[]; }
interface DoDCheck { name: string; run: string; }
interface DefinitionOfDone { checks: DoDCheck[]; }
interface Improvement { action: string; timing: ImprovementTiming; status: ImprovementStatus; outcome: string | null; }
interface Retrospective { sprint: number; improvements: Improvement[]; }
interface ScrumDashboard { product_goal: ProductGoal; product_backlog: PBI[]; sprint: Sprint | null; definition_of_done: DefinitionOfDone; completed: Sprint[]; retrospectives: Retrospective[]; }

// JSON output (deno run scrum.ts | jq for queries)
console.log(JSON.stringify(scrum, null, 2));
