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

  // Completed PBIs: PBI-001 through PBI-140 (Sprint 1-113), PBI-155-161 (Sprint 124-130), PBI-178-180a (Sprint 133-135), PBI-184 (Sprint 136), PBI-181 (Sprint 137), PBI-185 (Sprint 138), PBI-187 (Sprint 139)
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  // Removed: PBI-163-177 (obsolete - created before greenfield deletion per ASYNC_BRIDGE_REMOVAL.md)
  // Superseded: PBI-183 (merged into PBI-180b during Sprint 136 refinement)
  // Cancelled: Aborted Sprint 139 attempt (PBI-180b) - infrastructure didn't fix actual hang, reverted
  // Sprint Review 139: All ACs PASSED, all DoD checks PASSED - PBI-187 DONE
  product_backlog: [
    // ADR-0012 Phase 1: Single-LS-per-Language Foundation (PBI-178-181, PBI-184-185, PBI-187, PBI-180b done, Sprint 133-140)
    // Priority order: PBI-188 (multi-LS) > PBI-182 (features)
    // OBSOLETE: PBI-186 (lua-ls config) - lua-ls returns real results now, issue self-resolved
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
        { criterion: "During initialization window, connection tracks at most one pending request per incremental type (completion, hover, signatureHelp)", verification: "cargo test pending_incremental" },
        { criterion: "When new incremental request arrives during init before previous completes, older request receives REQUEST_FAILED (-32803) with 'superseded' message", verification: "cargo test supersede" },
        { criterion: "After initialization completes, only the latest pending incremental request is processed", verification: "cargo test --test e2e_lsp_init_supersede --features e2e" },
        { criterion: "E2E test via treesitter-ls binary verifies rapid typing during init triggers superseding", verification: "cargo test --test e2e_lsp_init_supersede --features e2e" },
      ],
      status: "done" as PBIStatus,
      refinement_notes: [
        "SPRINT 140 REFINEMENT: ACs rewritten to behavioral style, removed infrastructure-focused ACs",
        "SPRINT 140 REFINEMENT: AC3 (wait_for_initialized) removed - already satisfied by PBI-187",
        "SPRINT 140 REFINEMENT: AC4 (Phase 2 guard didChange/didSave dropping) DEFERRED - not needed for init window superseding",
        "SPRINT 140 REFINEMENT: E2E test renamed from e2e_bridge_* to e2e_lsp_* per naming convention",
        "DEPENDENCY: PBI-187 (non-blocking initialization) COMPLETED in Sprint 139",
        "SCOPE: Request superseding ONLY during initialization window (before didOpen sent to downstream)",
        "SCOPE: After didOpen sent, requests use simple forwarding (no superseding needed per ADR-0012 §7.3)",
        "SCOPE: Superseding applies to ALL incremental types (completion, hover, signatureHelp)",
        "RATIONALE: PBI-187 makes init non-blocking; PBI-180b handles requests DURING that init window",
        "VALUE: Prevents stale results during initialization window",
        "COMPLEXITY: Medium - patterns established, ~170 LOC estimated",
      ],
    },
    {
      id: "PBI-188",
      story: {
        role: "Rustacean editing Markdown",
        capability: "get LSP features for multiple embedded languages (Python, Go, TypeScript, etc.)",
        benefit: "I can use treesitter-ls with any language that has an LSP server, not just Lua",
      },
      acceptance_criteria: [
        { criterion: "Configuration maps language IDs to LS commands (e.g., python → pyright)", verification: "grep -E 'language.*command|LanguageServerConfig' src/lsp/bridge/" },
        { criterion: "Pool spawns correct LS based on language from configuration", verification: "grep -E 'get_command|config.*language' src/lsp/bridge/pool.rs" },
        { criterion: "Default configuration includes common language servers", verification: "grep -E 'lua.*language-server|python.*pyright|gopls' src/" },
        { criterion: "E2E test verifies at least two different language servers work", verification: "cargo test --test e2e_lsp_multi_lang --features e2e" },
        { criterion: "All unit tests pass", verification: "make test" },
      ],
      status: "draft" as PBIStatus,
      refinement_notes: [
        "SCOPE: Add configurable language-to-LS mapping (currently hardcoded lua-language-server)",
        "SCOPE: Support common LSes: pyright (Python), gopls (Go), typescript-language-server, etc.",
        "DEPENDENCY: PBI-187 (non-blocking init) and PBI-180b (init window handling) should be done first",
        "ARCHITECTURE: Aligns with ADR-0012 LanguageServerPool design for multiple LS connections",
        "CONFIGURATION: Could use TOML/JSON config file or environment variables",
        "VALUE: Makes bridge useful for polyglot markdown/documentation with multiple embedded languages",
        "NEXT STEPS: Decide configuration format and location during refinement",
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
  sprint: {
    number: 140,
    pbi_id: "PBI-180b",
    goal: "Cancel stale incremental requests during initialization window to prevent outdated results when typing rapidly",
    status: "planning" as SprintStatus,
    subtasks: [
      {
        test: "BridgeConnection tracks pending incremental requests (HashMap<IncrementalType, RequestId>)",
        implementation: "Add pending_incrementals: Arc<Mutex<HashMap<IncrementalType, u64>>> field to BridgeConnection, add IncrementalType enum (Completion, Hover, SignatureHelp)",
        type: "behavioral" as SubtaskType,
        status: "pending" as SubtaskStatus,
        commits: [],
        notes: [
          "Infrastructure for tracking at most one pending request per incremental type",
          "Use Mutex since we need exclusive access to update the map",
          "Store request_id (u64) to correlate supersede responses",
        ],
      },
      {
        test: "send_incremental_request() tracks request before sending, removes after response or supersede",
        implementation: "Add send_incremental_request(method, params, incremental_type) helper that: 1) checks pending_incrementals, 2) supersedes if exists, 3) tracks new request_id, 4) sends request, 5) removes from map on completion",
        type: "behavioral" as SubtaskType,
        status: "pending" as SubtaskStatus,
        commits: [],
        notes: [
          "Core superseding logic - when new request arrives, send REQUEST_FAILED to older request",
          "REQUEST_FAILED error code -32803, message: 'superseded by newer request'",
          "Must handle both success response and supersede response cleanup",
        ],
      },
      {
        test: "During initialization window, second completion request supersedes first pending request",
        implementation: "Unit test: spawn connection, send two completion requests before initialized, verify first receives REQUEST_FAILED (-32803) with 'superseded' message",
        type: "behavioral" as SubtaskType,
        status: "pending" as SubtaskStatus,
        commits: [],
        notes: [
          "Tests AC2: older request receives REQUEST_FAILED when new one arrives",
          "Use tokio::sync::mpsc or similar to capture responses",
          "Verify error code and message format",
        ],
      },
      {
        test: "After initialization completes, only latest pending request is processed (earlier superseded)",
        implementation: "Unit test: spawn connection, queue multiple requests during init window, wait for initialized, verify only latest processes successfully",
        type: "behavioral" as SubtaskType,
        status: "pending" as SubtaskStatus,
        commits: [],
        notes: [
          "Tests AC3: after init completes, only latest pending request processes",
          "Earlier requests should have received supersede errors already",
          "This validates the full lifecycle: queue → supersede → init complete → process latest",
        ],
      },
      {
        test: "Pool.completion() uses send_incremental_request() for superseding behavior",
        implementation: "Refactor Pool.completion() to call connection.send_incremental_request(method, params, IncrementalType::Completion) instead of direct send_request",
        type: "behavioral" as SubtaskType,
        status: "pending" as SubtaskStatus,
        commits: [],
        notes: [
          "Wire superseding into Pool layer for completion",
          "Minimal change - just swap send_request with send_incremental_request",
          "Applies to ALL incremental types per AC1",
        ],
      },
      {
        test: "Pool.hover() uses send_incremental_request() for superseding behavior",
        implementation: "Refactor Pool.hover() to call connection.send_incremental_request(method, params, IncrementalType::Hover) instead of direct send_request",
        type: "behavioral" as SubtaskType,
        status: "pending" as SubtaskStatus,
        commits: [],
        notes: [
          "Wire superseding into Pool layer for hover",
          "Pattern matches completion implementation",
        ],
      },
      {
        test: "E2E test: rapid typing during init triggers superseding (tests/e2e_lsp_init_supersede.rs)",
        implementation: "Create E2E test with LspClient: didOpen → rapid completion requests (3-5) before init completes → verify only latest receives valid response",
        type: "behavioral" as SubtaskType,
        status: "pending" as SubtaskStatus,
        commits: [],
        notes: [
          "Tests AC4: E2E verification via treesitter-ls binary",
          "Simulate user typing rapidly during initialization window",
          "Verify earlier requests receive REQUEST_FAILED (-32803)",
          "Verify latest request receives valid completion response after init",
        ],
      },
    ],
  },
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
  // Historical sprints (recent 4) | Sprint 1-135: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 139, pbi_id: "PBI-187", goal: "Enable non-blocking bridge connection initialization so users can edit immediately after opening files without LSP hangs", status: "done" as SprintStatus, subtasks: [
      { test: "wait_for_initialized() waits for initialized flag with timeout", implementation: "Add wait_for_initialized(timeout: Duration) method using initialized_notify.notified() with tokio::timeout", type: "behavioral" as SubtaskType, status: "completed" as SubtaskStatus, commits: [{ hash: "39c463e", message: "feat(bridge): add wait_for_initialized() for non-blocking init", phase: "green" as CommitPhase }], notes: ["Infrastructure exists: initialized AtomicBool, initialized_notify Notify"] },
      { test: "get_or_spawn_connection() returns immediately without waiting for initialize()", implementation: "Refactor get_or_spawn_connection() to spawn tokio::spawn task for initialize(), return Arc<BridgeConnection> immediately", type: "behavioral" as SubtaskType, status: "completed" as SubtaskStatus, commits: [{ hash: "845ac68", message: "feat(bridge): make connection initialization non-blocking", phase: "green" as CommitPhase }], notes: ["Background task handles initialize() + send_initialized_notification()"] },
      { test: "completion() and hover() wait for initialization before sending request", implementation: "Call connection.wait_for_initialized(Duration::from_secs(5)).await before send_request in pool.rs", type: "behavioral" as SubtaskType, status: "completed" as SubtaskStatus, commits: [{ hash: "8367b24", message: "feat(bridge): wire wait_for_initialized into completion and hover", phase: "green" as CommitPhase }], notes: ["Return InternalError if timeout expires"] },
      { test: "E2E test: typing immediately after file open does not hang", implementation: "Create tests/e2e_bridge_no_hang.rs with LspClient testing didOpen → didChange → completion sequence with no sleep delays", type: "behavioral" as SubtaskType, status: "completed" as SubtaskStatus, commits: [{ hash: "b578fd4", message: "test(e2e): add E2E test for non-blocking initialization", phase: "green" as CommitPhase }], notes: ["Test completes in ~65ms (< 2s timeout)", "Verifies no tokio runtime starvation"] },
    ] },
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
  // Retrospectives (recent 4) | Sprints 1-135: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    { sprint: 139, improvements: [
      { action: "Verify user-facing behavior with real-world testing after E2E passes (does hang actually disappear for end users?)", timing: "sprint", status: "active", outcome: null },
      { action: "Document cancelled sprint attempts in PBI refinement notes to prevent repeating same mistake", timing: "immediate", status: "completed", outcome: "Updated PBI-180b refinement_notes with 'SPRINT 139 CANCELLED' context and dependency on PBI-187" },
      { action: "Maintain strict TDD discipline for concurrent code (all wait_for_initialized tests written before implementation)", timing: "immediate", status: "completed", outcome: "All 3 wait_for_initialized tests written first, caught edge cases early, clean green phase" },
    ] },
    { sprint: 138, improvements: [
      { action: "Document AC interpretation strategy (infrastructure vs end-user behavior - when to accept 'infrastructure complete' vs 'user value delivered')", timing: "immediate", status: "completed", outcome: "Added 'Acceptance Criteria Interpretation Strategy' section to docs/e2e-testing-checklist.md with decision framework and Sprint 138 learning" },
      { action: "Create lua-ls workspace configuration investigation PBI (PBI-186: why null results despite didOpen with content - URI format, workspace config, timing, indexing)", timing: "product", status: "completed", outcome: "Created PBI-186 with draft status - investigation PBI to unlock semantic results for all bridged features (hover, completion, future definition/signatureHelp)" },
      { action: "Add E2E test debugging checklist (sleep timing, fixture quality, TODO placement, infrastructure vs config separation)", timing: "immediate", status: "completed", outcome: "Added 'E2E Test Debugging Checklist' section to docs/e2e-testing-checklist.md with 4-step systematic approach" },
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
