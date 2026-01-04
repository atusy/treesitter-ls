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

  // Completed PBIs: PBI-001 through PBI-140 (Sprint 1-113), PBI-155-161 (Sprint 124-130), PBI-178-180a (Sprint 133-135), PBI-184 (Sprint 136), PBI-181 (Sprint 137), PBI-185 (Sprint 138)
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  // Removed: PBI-163-177 (obsolete - created before greenfield deletion per ASYNC_BRIDGE_REMOVAL.md)
  // Superseded: PBI-183 (merged into PBI-180b during Sprint 136 refinement)
  // Cancelled: Sprint 139 (PBI-180b) - infrastructure didn't fix actual hang, reverted
  product_backlog: [
    // ADR-0012 Phase 1: Single-LS-per-Language Foundation (PBI-178-181, PBI-184-185 done, Sprint 133-138)
    // CRITICAL: PBI-187 must be done FIRST - it fixes the actual hang by making init non-blocking
    // Priority order: PBI-187 (hang fix) > PBI-180b (request handling during init) > PBI-182 (features)
    // OBSOLETE: PBI-186 (lua-ls config) - lua-ls returns real results now, issue self-resolved
    {
      id: "PBI-187",
      story: {
        role: "developer editing Lua files",
        capability: "edit files immediately after opening without LSP hanging",
        benefit: "I can work fluidly without waiting for bridge initialization to complete",
      },
      acceptance_criteria: [
        { criterion: "get_or_spawn_connection() returns immediately without blocking on initialize()", verification: "grep -A10 'get_or_spawn_connection' src/lsp/bridge/pool.rs | grep -v 'initialize().await'" },
        { criterion: "Connection initialization runs in background task (tokio::spawn)", verification: "grep -E 'tokio::spawn.*initialize|spawn.*init' src/lsp/bridge/pool.rs" },
        { criterion: "Requests to uninitialized connections return REQUEST_FAILED with clear message", verification: "grep -E 'REQUEST_FAILED.*not.*initialized|initializing' src/lsp/bridge/" },
        { criterion: "E2E test: typing immediately after file open does not hang", verification: "cargo test --test e2e_bridge_no_hang --features e2e" },
        { criterion: "All unit tests pass", verification: "make test" },
      ],
      status: "ready" as PBIStatus,
      refinement_notes: [
        "SPRINT 139 CANCELLED: PBI-180b built infrastructure but didn't fix actual hang",
        "ROOT CAUSE: get_or_spawn_connection() calls initialize().await synchronously",
        "ROOT CAUSE: When user types immediately, completion triggers init which blocks tokio runtime",
        "ROOT CAUSE: This starves other tasks (did_change) causing apparent hang",
        "FIX: Move initialization to background task, return early for requests during init",
        "SCOPE: Only make initialization non-blocking - request handling during init is PBI-180b",
        "DEPENDENCY: None - this is the foundation that PBI-180b depends on",
        "VALUE: Critical UX fix - users can edit immediately without waiting for bridge",
      ],
    },
    // PBI-186: OBSOLETE - lua-ls returns real results (hover shows types, completion works)
    // User confirmed hover shows: (global) x: { [1]: string = "x" }
    // The null results issue from Sprint 138 was likely a timing issue that resolved itself
    {
      id: "PBI-180b",
      story: {
        role: "developer editing Lua files",
        capability: "have stale incremental requests cancelled when typing rapidly during initialization",
        benefit: "I only see relevant suggestions for current code, not outdated results from earlier positions",
      },
      acceptance_criteria: [
        { criterion: "PendingIncrementalRequests struct tracks latest completion/hover/signatureHelp per connection", verification: "grep 'struct PendingIncrementalRequests' src/lsp/bridge/connection.rs" },
        { criterion: "Request superseding: newer incremental request cancels older with REQUEST_FAILED and superseded reason", verification: "grep -E 'register_completion|register_hover|REQUEST_FAILED.*superseded' src/lsp/bridge/connection.rs" },
        { criterion: "wait_for_initialized() with bounded timeout (5s) returns error if init doesn't complete", verification: "grep -B5 -A5 'wait_for_initialized' src/lsp/bridge/connection.rs" },
        { criterion: "Phase 2 guard: document notifications (didChange, didSave) dropped before didOpen sent", verification: "grep -B5 -A5 'did_open_sent' src/lsp/bridge/connection.rs | grep 'didChange\\|didSave'" },
        { criterion: "E2E test verifies rapid completion requests trigger superseding with only latest processed", verification: "cargo test --test e2e_bridge_init_supersede --features e2e" },
        { criterion: "All unit tests pass with superseding infrastructure", verification: "make test" },
      ],
      status: "draft" as PBIStatus,
      refinement_notes: [
        "SPRINT 139 CANCELLED: Built infrastructure without fixing root cause (blocking init)",
        "DEPENDENCY: Requires PBI-187 (non-blocking initialization) - BLOCKED until PBI-187 done",
        "SCOPE: Request superseding during initialization window only (ADR-0012 §7.3)",
        "SCOPE: Phase 2 guard implementation (wait pattern + document notification dropping)",
        "RATIONALE: PBI-187 makes init non-blocking; PBI-180b handles requests DURING that init window",
        "VALUE: Prevents stale results during initialization window (§7.3: after didOpen, requests simply forward)",
        "COMPLEXITY: Medium - patterns are clear, just need PBI-187 foundation first",
        "LEARNING: Sprint 139 showed that request handling during init is useless if init itself blocks",
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
        { criterion: "Pool.definition() routes to appropriate connection and returns translated response", verification: "grep -E 'fn definition|pub async fn definition' src/lsp/bridge/pool.rs" },
        { criterion: "Pool.signature_help() routes to appropriate connection and returns signature response", verification: "grep -E 'fn signature_help|pub async fn signature_help' src/lsp/bridge/pool.rs" },
        { criterion: "E2E test verifies goto definition returns real locations from lua-ls", verification: "cargo test --test e2e_lsp_lua_definition --features e2e" },
        { criterion: "E2E test verifies signature help shows real function signatures", verification: "cargo test --test e2e_lsp_lua_signature --features e2e" },
      ],
      status: "draft" as PBIStatus,
      refinement_notes: [
        "SPRINT 139 REFINEMENT: ACs corrected for ADR-0012 alignment (Pool layer, binary-first E2E naming)",
        "SPRINT 137 REFINEMENT: Kept as draft - ACs needed correction before ready",
        "CONSIDERATION: May split into PBI-182a (definition) and PBI-182b (signatureHelp) - two distinct features",
        "DEPENDENCY: Infrastructure exists (PBI-180a, PBI-184) - technical readiness confirmed",
        "DEPENDENCY: PBI-186 (lua-ls config) should be resolved first for real semantic results",
        "NEXT STEPS: Promote to ready after PBI-186 confirms lua-ls responds to requests",
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
    ],
  },
  // Historical sprints (recent 3) | Sprint 1-135: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 138, pbi_id: "PBI-185", goal: "Virtual document synchronization - send didOpen with content before LSP requests", status: "done", subtasks: [
      { test: "Track opened documents with HashSet", implementation: "opened_documents: Arc<Mutex<HashSet<String>>> in BridgeConnection", type: "behavioral", status: "completed", commits: [{ hash: "c1b2c2e", message: "feat(bridge): track opened virtual documents", phase: "green" }], notes: [] },
      { test: "Idempotent check_and_send_did_open()", implementation: "Check HashSet, send didOpen if not present, add to set", type: "behavioral", status: "completed", commits: [{ hash: "e1a1799", message: "feat(bridge): implement check_and_send_did_open", phase: "green" }], notes: [] },
      { test: "Wire completion/hover to send didOpen with content", implementation: "Extract content via cacheable.extract_content(), call check_and_send_did_open before send_request", type: "behavioral", status: "completed", commits: [{ hash: "53d3608", message: "feat(bridge): wire completion", phase: "green" }, { hash: "ac7b075", message: "feat(bridge): wire hover", phase: "green" }], notes: ["Infrastructure complete, lua-ls returns null (config issue→PBI-186)"] },
    ] },
    { number: 137, pbi_id: "PBI-181", goal: "Hover support for Lua code blocks in markdown", status: "done", subtasks: [
      { test: "Pool.hover() implementation following completion pattern", implementation: "Extract language, spawn connection, send textDocument/hover, deserialize response", type: "behavioral", status: "completed", commits: [{ hash: "7921b6c", message: "feat(bridge): implement hover support (PBI-181)", phase: "green" }], notes: [] },
      { test: "hover_impl() wires Pool.hover() with virtual URI", implementation: "Translate position, build HoverParams with virtual URI", type: "behavioral", status: "completed", commits: [{ hash: "7921b6c", message: "feat(bridge): implement hover support (PBI-181)", phase: "green" }], notes: [] },
      { test: "E2E test via LspClient (treesitter-ls binary)", implementation: "tests/e2e_lsp_lua_hover.rs: hover over print in Lua block", type: "behavioral", status: "completed", commits: [{ hash: "7921b6c", message: "feat(bridge): implement hover support (PBI-181)", phase: "green" }], notes: ["Pattern reuse from e2e_lsp_lua_completion.rs"] },
    ] },
    { number: 136, pbi_id: "PBI-184", goal: "Wire bridge infrastructure to treesitter-ls binary with connection lifecycle management", status: "done", subtasks: [
      { test: "DashMap<String, Arc<BridgeConnection>> for per-language connections", implementation: "Replace _connection: Option with DashMap for concurrent access", type: "behavioral", status: "completed", commits: [{ hash: "1c61df2", message: "feat(bridge): add DashMap for per-language connections", phase: "green" }], notes: [] },
      { test: "get_or_spawn_connection(language) spawns on first access", implementation: "DashMap entry API with lazy initialization", type: "behavioral", status: "completed", commits: [{ hash: "df04faf", message: "feat(bridge): implement lazy connection spawning", phase: "green" }], notes: [] },
      { test: "Pool.completion() uses real send_request", implementation: "Extract language from URI, spawn connection, forward request", type: "behavioral", status: "completed", commits: [{ hash: "d3fbae8", message: "feat(bridge): wire completion to real send_request", phase: "green" }], notes: [] },
      { test: "E2E test via LspClient (treesitter-ls binary)", implementation: "tests/e2e_lsp_lua_completion.rs with correct E2E pattern", type: "behavioral", status: "completed", commits: [{ hash: "797ad7a", message: "feat(bridge): add proper E2E test via LspClient", phase: "green" }], notes: ["lua-ls returns null until didOpen with virtual content implemented"] },
      { test: "Deprecate wrong-layer e2e_bridge tests", implementation: "Add deprecation comments pointing to correct pattern", type: "structural", status: "completed", commits: [{ hash: "5c26c45", message: "docs: deprecate wrong-layer E2E tests", phase: "green" }], notes: [] },
    ] },
  ],
  // Retrospectives (recent 3) | Sprints 1-135: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    { sprint: 139, improvements: [
      { action: "Identify root cause BEFORE building infrastructure (Sprint 139 built request superseding but actual hang was in blocking initialize())", timing: "immediate", status: "completed", outcome: "Created PBI-187 for non-blocking initialization; PBI-180b demoted to draft, blocked by PBI-187" },
      { action: "Add 'does this fix the actual problem?' checkpoint to Sprint Review (ACs passed but user problem persisted)", timing: "immediate", status: "completed", outcome: "Sprint 139 cancelled and reverted when review revealed infrastructure didn't fix hang" },
      { action: "Trace hang through full call stack before proposing solution (completion→get_or_spawn→initialize→blocking loop)", timing: "sprint", status: "active", outcome: null },
    ] },
    { sprint: 138, improvements: [
      { action: "Document AC interpretation strategy (infrastructure vs end-user behavior - when to accept 'infrastructure complete' vs 'user value delivered')", timing: "immediate", status: "completed", outcome: "Added 'Acceptance Criteria Interpretation Strategy' section to docs/e2e-testing-checklist.md with decision framework and Sprint 138 learning" },
      { action: "Create lua-ls workspace configuration investigation PBI (PBI-186: why null results despite didOpen with content - URI format, workspace config, timing, indexing)", timing: "product", status: "completed", outcome: "Created PBI-186 with draft status - investigation PBI to unlock semantic results for all bridged features (hover, completion, future definition/signatureHelp)" },
      { action: "Add E2E test debugging checklist (sleep timing, fixture quality, TODO placement, infrastructure vs config separation)", timing: "immediate", status: "completed", outcome: "Added 'E2E Test Debugging Checklist' section to docs/e2e-testing-checklist.md with 4-step systematic approach" },
    ] },
    { sprint: 137, improvements: [
      { action: "Create virtual document synchronization tracking system (track didOpen/didChange per connection, send virtual content on first access)", timing: "product", status: "completed", outcome: "Created PBI-185 with ready status - highest priority for Sprint 138 due to user-facing bug (hover/completion return null without didOpen content)" },
      { action: "Consolidate granular subtasks in sprint planning (11 subtasks in Sprint 137 could be 5-6 higher-level tasks, apply ADR-0012 PBI splitting criteria)", timing: "sprint", status: "active", outcome: null },
      { action: "Document pattern reuse strategy in ADR-0012 (completion→hover→definition progression validates incremental feature rollout)", timing: "sprint", status: "active", outcome: null },
    ] },
    { sprint: 136, improvements: [
      { action: "Create E2E testing anti-pattern checklist (test through binary not library, must use LspClient, verification criteria for E2E tests)", timing: "immediate", status: "completed", outcome: "Created docs/e2e-testing-checklist.md documenting binary-first principle and verification criteria from Sprint 136 experience" },
      { action: "Assess deprecated e2e_bridge tests for removal (e2e_bridge_completion.rs tests wrong layer, e2e_bridge_fakeit.rs tests obsolete phase)", timing: "immediate", status: "completed", outcome: "Kept deprecated tests with clear deprecation comments - provide historical documentation value; e2e_lsp_lua_completion.rs is now canonical E2E pattern" },
      { action: "Add 'wire early' principle to ADR-0012 (wire infrastructure incrementally vs batch at end, reduces feedback delay)", timing: "sprint", status: "active", outcome: null },
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
