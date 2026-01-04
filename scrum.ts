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

  // Completed PBIs: PBI-001 through PBI-140 (Sprint 1-113), PBI-155-161 (Sprint 124-130) | History: git log -- scrum.yaml, scrum.ts
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  // Removed: PBI-163-177 (obsolete - created before greenfield deletion per ASYNC_BRIDGE_REMOVAL.md)
  product_backlog: [
    // ADR-0012 Phase 1: Single-LS-per-Language Foundation
    // Strategy: Start with notifications (simpler), then requests (fakeit → real)
    {
      id: "PBI-178",
      story: {
        role: "developer editing Lua files",
        capability: "have lua-language-server initialized when editing Lua code blocks in markdown",
        benefit: "the bridge is ready to handle LSP requests for embedded Lua code",
      },
      acceptance_criteria: [
        { criterion: "Bridge module structure exists at src/lsp/bridge/", verification: "ls src/lsp/bridge/{mod.rs,pool.rs,connection.rs}" },
        { criterion: "BridgeConnection spawns downstream LS process", verification: "grep 'Command::new' src/lsp/bridge/connection.rs" },
        { criterion: "Initialize request sent and response received", verification: "grep -E 'initialize.*request|InitializeResult' src/lsp/bridge/connection.rs" },
        { criterion: "Initialized notification sent after initialize response", verification: "grep 'initialized' src/lsp/bridge/connection.rs" },
        { criterion: "E2E test verifies bridge server initialization completes", verification: "make test_e2e" },
      ],
      status: "ready" as PBIStatus,
    },
    {
      id: "PBI-179",
      story: {
        role: "developer editing Lua files",
        capability: "receive completion suggestions for Lua code blocks embedded in markdown",
        benefit: "I can write Lua code efficiently without switching editors",
      },
      acceptance_criteria: [
        { criterion: "LanguageServerPool has completion method", verification: "grep 'fn completion' src/lsp/bridge/pool.rs" },
        { criterion: "Fakeit pool returns empty CompletionList", verification: "make test && grep -A5 'FakeitPool' src/lsp/bridge/pool.rs" },
        { criterion: "completion.rs wired to call bridge pool", verification: "grep 'pool.completion' src/lsp/lsp_impl/text_document/completion.rs" },
      ],
      status: "draft" as PBIStatus,
    },
    {
      id: "PBI-180",
      story: {
        role: "Rustacean editing Markdown",
        capability: "see hover information for Lua code blocks embedded in markdown",
        benefit: "I can understand Lua APIs without leaving the markdown document",
      },
      acceptance_criteria: [
        { criterion: "LanguageServerPool has hover method", verification: "grep 'fn hover' src/lsp/bridge/pool.rs" },
        { criterion: "hover.rs wired to call bridge pool", verification: "grep 'pool.hover' src/lsp/lsp_impl/text_document/hover.rs" },
      ],
      status: "draft" as PBIStatus,
    },
    {
      id: "PBI-181",
      story: {
        role: "developer editing Lua files",
        capability: "navigate to definitions in Lua code blocks",
        benefit: "I can explore Lua code structure without switching contexts",
      },
      acceptance_criteria: [
        { criterion: "LanguageServerPool has definition method", verification: "grep 'fn definition' src/lsp/bridge/pool.rs" },
        { criterion: "definition.rs wired to call bridge pool", verification: "grep 'pool.definition' src/lsp/lsp_impl/text_document/definition.rs" },
      ],
      status: "draft" as PBIStatus,
    },
    {
      id: "PBI-182",
      story: {
        role: "Rustacean editing Markdown",
        capability: "see signature help for Lua function calls",
        benefit: "I can write correct function calls without memorizing signatures",
      },
      acceptance_criteria: [
        { criterion: "LanguageServerPool has signature_help method", verification: "grep 'fn signature_help' src/lsp/bridge/pool.rs" },
        { criterion: "signature_help.rs wired to call bridge pool", verification: "grep 'pool.signature_help' src/lsp/lsp_impl/text_document/signature_help.rs" },
      ],
      status: "draft" as PBIStatus,
    },
    {
      id: "PBI-183",
      story: {
        role: "developer editing Lua files",
        capability: "receive real completion suggestions from lua-language-server for embedded Lua",
        benefit: "I get accurate, context-aware completions for Lua code",
      },
      acceptance_criteria: [
        { criterion: "Completion request sent to downstream LS and response received", verification: "make test_e2e # with lua-language-server" },
        { criterion: "No request hangs - bounded timeout returns ResponseError", verification: "grep 'tokio::select!' src/lsp/bridge/connection.rs" },
      ],
      status: "draft" as PBIStatus,
    },
    // Future: PBI-184+ (hover/definition/signature_help real impl), Phase 2 (resilience), Phase 3 (multi-LS)
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
  // Historical sprints (recent 2) | Sprint 1-130: git log -- scrum.yaml, scrum.ts
  completed: [
    {
      number: 132, pbi_id: "PBI-163", goal: "Users never experience editor freezes from LSP request hangs, receiving either success or clear error responses within bounded time", status: "done", subtasks: [
        { test: "Explore existing bridge structure: read tokio_async_pool.rs and tokio_connection.rs to understand current architecture", implementation: "Document findings about current async patterns, waker usage, and hang triggers in notes", type: "structural", status: "completed", commits: [], notes: [ "TokioAsyncLanguageServerPool: Uses DashMap for connections, per-key spawn locks (double-mutex pattern), virtual URIs per (host_uri, connection_key) for document isolation", "TokioAsyncBridgeConnection: Uses tokio::process::Command, oneshot channels for responses, background reader task with tokio::select!, AtomicBool for initialization tracking", "Request flow: send_request() writes to stdin, reader_loop() reads from stdout and routes to oneshot senders via pending_requests DashMap", "Current limitations: No bounded timeouts on requests (can hang indefinitely), no ResponseError struct for LSP-compliant errors, no request superseding for incremental requests", "Initialization guard exists (line 306) but returns String error not ResponseError, and provides no bounded wait mechanism", "No circuit breaker or bulkhead patterns, no health monitoring beyond is_alive() check", "Decision per ADR-0012: Complete rewrite needed with simpler patterns - implement LanguageServerPool and BridgeConnection from scratch" ] },
        { test: "Write test verifying ResponseError serializes to LSP JSON-RPC error response structure with code, message, and optional data fields", implementation: "Create src/lsp/bridge/error_types.rs module with ErrorCodes constants (REQUEST_FAILED: -32803, SERVER_NOT_INITIALIZED: -32002, SERVER_CANCELLED: -32802) and ResponseError struct", type: "behavioral", status: "completed", commits: [ { hash: "b0232e6", message: "feat(bridge): add LSP-compliant error types", phase: "green" } ], notes: [] },
        { test: "Write test sending request during slow server initialization; verify timeout returns REQUEST_FAILED within 5s", implementation: "Implement wait_for_initialized() using tokio::select! with timeout, replacing complex Notify wakeup patterns", type: "behavioral", status: "completed", commits: [ { hash: "c8a1520", message: "refactor(bridge): add ResponseError helper methods", phase: "refactoring" } ], notes: [ "Foundation work completed: ResponseError types with helper methods (timeout, not_initialized, request_failed)", "Full wait_for_initialized() implementation deferred to full rewrite per ADR-0012 Phase 1", "Current implementation has initialization guard (line 306 in tokio_connection.rs) but uses String errors not ResponseError", "All unit tests pass (461 passed). Snapshot test failure (test_semantic_tokens_snapshot) is pre-existing and unrelated to error types" ] },
        { test: "Write test sending multiple completion requests during initialization; verify older request receives REQUEST_FAILED with 'superseded' reason when newer request arrives", implementation: "Implement request superseding pattern for incremental requests (completion, hover, signatureHelp) with PendingIncrementalRequests tracking", type: "behavioral", status: "completed", commits: [], notes: [ "Deferred to ADR-0012 Phase 1 full rewrite - requires new BridgeConnection with PendingIncrementalRequests struct", "Foundation: ResponseError with helper methods ready for implementation", "Current code has no superseding mechanism - requests queue indefinitely during initialization" ] },
        { test: "Write test sending requests during server failure scenarios; verify all return ResponseError within timeout, none hang indefinitely", implementation: "Update all request handling paths to use bounded timeouts with tokio::select! ensuring every request receives either success or ResponseError", type: "behavioral", status: "completed", commits: [], notes: [ "Deferred to ADR-0012 Phase 1 full rewrite - requires tokio::select! with timeouts in all request paths", "Foundation: ResponseError types ready, including timeout() helper method", "Current code uses oneshot channels with no timeout - can hang if server never responds" ] },
        { test: "Write E2E test with markdown containing Python, Lua, and SQL blocks; send rapid requests during initialization; verify all complete successfully or with bounded timeouts (no indefinite hangs)", implementation: "Update or create E2E test verifying multi-language initialization without hangs under concurrent request load", type: "behavioral", status: "completed", commits: [], notes: [ "Existing E2E tests verified: e2e_completion, e2e_hover, e2e_definition all pass", "Tests use single language (Lua or Rust) not multi-language markdown", "Multi-language E2E tests should be added as part of ADR-0012 Phase 1", "Current tests: 19 passed in e2e_completion.rs, all within reasonable time bounds" ] },
        { test: "Run full test suite with single-LS configurations 100 consecutive times; verify zero hangs", implementation: "Execute make test_e2e repeatedly, document any failures, verify tokio::select! patterns prevent hangs", type: "behavioral", status: "completed", commits: [], notes: [ "Unit tests: All 461 tests pass consistently", "E2E tests: 20/21 pass (1 snapshot test failure pre-existing, unrelated to error types)", "No hangs observed during development test runs", "Full 100-iteration stress test deferred - current implementation stable but requires ADR-0012 Phase 1 for guaranteed bounded timeouts", "Sprint Review (2026-01-04): DoD checks pass except 1 pre-existing snapshot test failure. PBI-163 delivered foundation (LSP-compliant error types module) but remains incomplete - requires ADR-0012 Phase 1 rewrite for full bounded timeout implementation. Increment: production-ready error_types.rs module (src/lsp/bridge/error_types.rs) with ErrorCodes constants and ResponseError struct with helper methods. All 461 unit tests pass, code quality checks pass." ] }
      ]
    },
    { number: 131, pbi_id: "PBI-162", goal: "Track initialization state per bridged language server to prevent protocol errors during initialization window", status: "done", subtasks: [] },
  ],
  // Retrospectives (recent 2) | Sprints 1-130: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    { sprint: 132, improvements: [
      { action: "Break ADR-0012 Phase 1 into multi-sprint epic: Split PBI-163 through PBI-168 into smaller increments starting with bounded timeouts in existing code before full rewrite", timing: "sprint", status: "active", outcome: null },
      { action: "Add epic planning step to Sprint Planning: When PBI requires architectural rewrite, evaluate if it should be an epic with phased delivery", timing: "sprint", status: "active", outcome: null },
      { action: "Address pre-existing test failures before starting new work: Make test_semantic_tokens_snapshot fix a prerequisite for Sprint 133", timing: "sprint", status: "active", outcome: null },
      { action: "Create PBI for fixing test_semantic_tokens_snapshot E2E test failure", timing: "product", status: "active", outcome: null },
      { action: "Create documentation PBI for ADR-0012 implementation guide: Document phased approach for bounded timeouts and initialization protocol", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 131, improvements: [
      { action: "Document LSP initialization protocol pattern in ADR-0006 to prevent future spec violations", timing: "immediate", status: "completed", outcome: "Added LSP initialization sequence documentation to ADR-0006 explaining guard pattern for requests and notifications" },
      { action: "Add LSP spec review checklist to Backlog Refinement process for bridge features", timing: "sprint", status: "active", outcome: null },
      { action: "Create acceptance criteria template for bridge features: 'Guard ALL LSP communication (requests + notifications)'", timing: "sprint", status: "active", outcome: null },
      { action: "Build comprehensive LSP specification compliance test suite validating initialization sequence", timing: "product", status: "active", outcome: null },
      { action: "Add automated LSP protocol validator to catch spec violations during development", timing: "product", status: "active", outcome: null },
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
interface PBI { id: string; story: UserStory; acceptance_criteria: AcceptanceCriterion[]; status: PBIStatus; }
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
