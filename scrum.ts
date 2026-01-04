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
    // Strategy: Two-pass approach: (1) Fakeit pass - all components with dummy responses, (2) Real pass - replace with actual LSP
    {
      id: "PBI-178",
      story: {
        role: "developer editing Lua files",
        capability: "have bridge infrastructure ready with fakeit responses for all LSP methods",
        benefit: "E2E tests pass with new API structure before implementing real async LSP communication",
      },
      acceptance_criteria: [
        { criterion: "Bridge module structure created with pool.rs, connection.rs, mod.rs", verification: "ls src/lsp/bridge/pool.rs src/lsp/bridge/connection.rs src/lsp/bridge/mod.rs" },
        { criterion: "LanguageServerPool trait/struct with completion, hover, definition, signature_help methods", verification: "grep 'fn completion\\|fn hover\\|fn definition\\|fn signature_help' src/lsp/bridge/pool.rs" },
        { criterion: "All LSP methods return Ok(None) or empty response structures (fakeit)", verification: "grep -A2 'fn completion' src/lsp/bridge/pool.rs | grep 'Ok(None)'" },
        { criterion: "BridgeConnection struct exists with stubbed spawn and initialize (no real process)", verification: "grep 'struct BridgeConnection' src/lsp/bridge/connection.rs" },
        { criterion: "completion.rs wired to call pool.completion() for injection regions", verification: "grep 'pool.completion\\|language_server_pool' src/lsp/lsp_impl/text_document/completion.rs" },
        { criterion: "E2E test sends completion to Lua block and receives Ok(None) without hanging", verification: "cargo test --test e2e_bridge_fakeit --features e2e" },
        { criterion: "All unit tests pass with new bridge structure", verification: "make test" },
      ],
      status: "done" as PBIStatus,
    },
    {
      id: "PBI-179",
      story: {
        role: "developer editing Lua files",
        capability: "have lua-language-server initialized when editing Lua code blocks in markdown",
        benefit: "the bridge is ready to handle real LSP requests for embedded Lua code",
      },
      acceptance_criteria: [
        { criterion: "BridgeConnection spawns actual lua-language-server process via tokio::process::Command", verification: "grep -E 'Command::new.*lua-language-server|process::Command' src/lsp/bridge/connection.rs" },
        { criterion: "Initialize request sent to lua-ls stdin and InitializeResult received from stdout", verification: "grep -E 'initialize.*request|InitializeResult|send_request.*initialize' src/lsp/bridge/connection.rs" },
        { criterion: "Initialized notification sent to lua-ls after receiving initialize response", verification: "grep -E 'initialized.*notification|send_notification.*initialized' src/lsp/bridge/connection.rs" },
        { criterion: "Phase 1 notification guard blocks notifications before initialized with SERVER_NOT_INITIALIZED", verification: "grep -B5 'SERVER_NOT_INITIALIZED' src/lsp/bridge/connection.rs | grep 'initialized.load'" },
        { criterion: "didOpen notification sent to lua-ls with virtual document URI and content", verification: "grep -E 'didOpen|textDocument/didOpen' src/lsp/bridge/connection.rs" },
        { criterion: "E2E test verifies lua-ls process spawned and initialization completes within 5s", verification: "cargo test --test e2e_bridge_init --features e2e" },
        { criterion: "All unit tests pass including initialization timeout handling", verification: "make test" },
      ],
      status: "done" as PBIStatus,
    },
    {
      id: "PBI-180",
      story: {
        role: "developer editing Lua files",
        capability: "receive real completion suggestions from lua-language-server for embedded Lua",
        benefit: "I can write Lua code efficiently with accurate, context-aware completions",
      },
      acceptance_criteria: [
        { criterion: "BridgeConnection.send_request implements textDocument/completion with request ID tracking and response correlation", verification: "grep -A10 'send_request' src/lsp/bridge/connection.rs | grep -E 'request_id|next_request_id'" },
        { criterion: "Completion request uses virtual document URI with translated position from host coordinates", verification: "grep -E 'virtual.*uri|translate.*position' src/lsp/lsp_impl/text_document/completion.rs" },
        { criterion: "Completion response ranges translated back to host document coordinates before returning", verification: "grep -E 'translate.*host|host.*range' src/lsp/lsp_impl/text_document/completion.rs" },
        { criterion: "Request superseding: newer completion cancels older with REQUEST_FAILED during init window", verification: "grep -E 'PendingIncrementalRequests|superseded|REQUEST_FAILED' src/lsp/bridge/connection.rs" },
        { criterion: "Bounded timeout (5s default, configurable) returns REQUEST_FAILED if lua-ls unresponsive", verification: "grep -E 'tokio::select!|timeout.*5|Duration::from_secs' src/lsp/bridge/connection.rs" },
        { criterion: "Phase 2 guard allows completion after initialized but before didOpen with wait pattern", verification: "grep -B5 -A5 'wait_for_initialized' src/lsp/bridge/connection.rs | grep 'send_request'" },
        { criterion: "E2E test sends completion to Lua block and receives real CompletionItems from lua-ls", verification: "cargo test --test e2e_bridge_completion --features e2e" },
        { criterion: "E2E test verifies rapid completion requests trigger superseding with only latest processed", verification: "cargo test --test e2e_bridge_init_race --features e2e" },
      ],
      status: "refining" as PBIStatus,
      refinement_notes: [
        "COMPLEXITY CONCERN: 8 acceptance criteria mixing infrastructure (send_request, superseding) with feature (completion)",
        "RECOMMENDATION: Split into PBI-180a (basic request/response) + PBI-180b (request superseding pattern)",
        "PBI-180a would deliver value (real completions work) in one sprint",
        "PBI-180b would add robustness (rapid typing scenarios) in follow-up sprint",
        "Alternative: Keep as-is if superseding is deemed essential for MVP user experience",
        "Decision needed: Sprint 135 planning should confirm scope before starting work",
      ],
    },
    {
      id: "PBI-181",
      story: {
        role: "developer editing Lua files",
        capability: "see hover information for Lua code in markdown code blocks",
        benefit: "I can understand Lua APIs and types without leaving the markdown document",
      },
      acceptance_criteria: [
        { criterion: "LanguageServerPool.hover() wired to call BridgeConnection.send_request with textDocument/hover", verification: "grep 'send_request.*hover\\|hover.*send_request' src/lsp/bridge/pool.rs" },
        { criterion: "Hover request uses virtual document URI and translated position from host coordinates", verification: "grep -E 'virtual.*uri|translate.*position' src/lsp/lsp_impl/text_document/hover.rs" },
        { criterion: "Hover response (Hover with contents) returned to host without range translation (hover ranges are optional)", verification: "grep -E 'pool.hover|Hover' src/lsp/lsp_impl/text_document/hover.rs" },
        { criterion: "Request superseding: newer hover cancels older with REQUEST_FAILED (reuses PendingIncrementalRequests from PBI-180)", verification: "grep -E 'register_hover|PendingIncrementalRequests' src/lsp/bridge/connection.rs" },
        { criterion: "E2E test receives real hover information from lua-ls for Lua built-in (e.g., print)", verification: "cargo test --test e2e_bridge_hover --features e2e" },
        { criterion: "All unit tests pass with hover implementation", verification: "make test" },
      ],
      status: "ready" as PBIStatus,
      refinement_notes: [
        "DEPENDENCY: Assumes PBI-180 (or PBI-180a if split) completes send_request and PendingIncrementalRequests infrastructure",
        "LEVERAGE: Reuses request superseding pattern from PBI-180, only adds hover-specific wiring",
        "SIMPLIFICATION: Hover ranges are optional in LSP spec; we can skip range translation for MVP",
        "COMPLEXITY: Lower than PBI-180 because infrastructure exists; mainly method-specific logic",
        "ESTIMATE: Should be smaller than PBI-180; could pair with another small PBI if needed",
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
    },
    {
      id: "PBI-183",
      story: {
        role: "developer editing Lua files",
        capability: "have stale completion requests cancelled when typing rapidly",
        benefit: "I only see relevant suggestions for current code, not outdated results",
      },
      acceptance_criteria: [
        { criterion: "PendingIncrementalRequests struct tracks latest completion/hover/signatureHelp per connection", verification: "grep 'struct PendingIncrementalRequests' src/lsp/bridge/connection.rs" },
        { criterion: "Older incremental requests receive REQUEST_FAILED with superseded reason when newer arrives", verification: "grep -A5 'superseded\\|REQUEST_FAILED' src/lsp/bridge/connection.rs | grep 'incremental'" },
        { criterion: "E2E test sends rapid completion requests and verifies only latest processed", verification: "cargo test --test e2e_bridge_superseding --features e2e" },
        { criterion: "No request hangs indefinitely - all timeouts enforced with tokio::select!", verification: "grep 'tokio::select!' src/lsp/bridge/connection.rs" },
      ],
      status: "draft" as PBIStatus,
    },
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
  // Historical sprints (recent 2) | Sprint 1-133: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 134, pbi_id: "PBI-179", goal: "Real LSP initialization: spawn lua-language-server, handle initialize protocol, and implement Phase 1 notification guard", status: "done", subtasks: [
      { test: "BridgeConnection spawns lua-language-server process with tokio::process::Command", implementation: "Replace stubbed new() with tokio::process::Command::new(\"lua-language-server\").stdin/stdout/stderr(Stdio::piped()).spawn()", type: "behavioral", status: "completed", commits: [{ hash: "ddc4875", message: "feat(bridge): spawn real language server process with tokio", phase: "green" }], notes: ["✓ Implemented async new() with tokio::process::Command", "✓ Added custom Debug impl for BridgeConnection", "✓ Tests pass for valid command and invalid command error handling"] },
      { test: "JSON-RPC message framing with Content-Length headers for stdio communication", implementation: "Add read_message/write_message helpers parsing Content-Length header + \\r\\n\\r\\n + JSON body", type: "behavioral", status: "completed", commits: [{ hash: "707496d", message: "feat(bridge): implement JSON-RPC message framing for LSP", phase: "green" }], notes: ["✓ Implemented write_message with AsyncWrite trait", "✓ Implemented read_message with AsyncRead trait and BufReader", "✓ All tests pass for valid framing and error cases"] },
      { test: "Initialize request sent and InitializeResult received from lua-language-server", implementation: "BridgeConnection::initialize() sends initialize request with clientInfo/capabilities, waits for InitializeResult response", type: "behavioral", status: "completed", commits: [{ hash: "2a5300e", message: "feat(bridge): implement LSP initialize request protocol", phase: "green" }], notes: ["✓ Implemented send_initialize_request() with request ID tracking", "✓ Wrapped stdin/stdout/process in Mutex for async access", "✓ Verifies response ID matches request ID"] },
      { test: "Initialized notification sent to lua-language-server after receiving initialize response", implementation: "After receiving InitializeResult, send initialized notification (no id, method='initialized', params={}), set initialized flag", type: "behavioral", status: "completed", commits: [{ hash: "5b9a516", message: "feat(bridge): implement initialized notification", phase: "green" }], notes: ["✓ Implemented send_initialized_notification()", "✓ Added initialized_notify Notify field for wait pattern", "✓ Sets initialized flag and triggers notify_waiters()"] },
      { test: "Phase 1 notification guard blocks notifications before initialized with SERVER_NOT_INITIALIZED", implementation: "BridgeConnection::send_notification checks initialized flag; return Err(SERVER_NOT_INITIALIZED) if false (except for 'initialized' method itself)", type: "behavioral", status: "completed", commits: [{ hash: "f276b53", message: "feat(bridge): implement Phase 1 notification guard", phase: "green" }], notes: ["✓ Implemented send_notification() with Phase 1 guard", "✓ Tests verify guard blocks before init, allows after init", "✓ Exception for 'initialized' method itself"] },
      { test: "didOpen notification sent to lua-language-server with virtual document URI and content", implementation: "Add send_did_open(uri, language_id, text) method sending textDocument/didOpen notification, set did_open_sent flag", type: "behavioral", status: "completed", commits: [{ hash: "844b40d", message: "feat(bridge): implement didOpen notification", phase: "green" }], notes: ["✓ Implemented send_did_open(uri, language_id, text)", "✓ Sets did_open_sent flag after successful send", "✓ Tests verify Phase 1 guard works and flag set correctly"] },
      { test: "Bounded timeout handling for initialization with tokio::select!", implementation: "Wrap initialize request/response in tokio::select! with tokio::time::sleep(Duration::from_secs(5)) timeout arm", type: "behavioral", status: "completed", commits: [{ hash: "79125fb", message: "feat(bridge): add initialize() with 5s timeout", phase: "green" }], notes: ["✓ Added initialize() convenience method with tokio::time::timeout", "✓ 5s timeout wraps send_initialize_request()", "✓ Auto-sends initialized notification after response"] },
      { test: "E2E test verifies real lua-language-server process spawned and initialization completes within 5s", implementation: "tests/e2e_bridge_init.rs: spawn real lua-ls, verify initialize → initialized → didOpen sequence completes, verify process alive", type: "behavioral", status: "completed", commits: [{ hash: "85393d6", message: "feat(bridge): add E2E test for real lua-language-server initialization", phase: "green" }], notes: ["✓ Created tests/e2e_bridge_init.rs with 3 E2E tests", "✓ Made bridge modules public with e2e feature gate", "✓ All E2E tests pass with real lua-language-server", "384 unit tests pass", "3 E2E init tests pass", "Initialization handshake < 200ms (well under 5s timeout)"] },
    ] },
    { number: 133, pbi_id: "PBI-178", goal: "Establish complete fakeit bridge infrastructure with E2E tests passing", status: "done", subtasks: [
      { test: "Create bridge module structure", implementation: "src/lsp/bridge/{mod,pool,connection}.rs with public exports", type: "structural", status: "completed", commits: [{ hash: "73a87f9", message: "feat(bridge): create bridge module structure", phase: "green" }], notes: [] },
      { test: "Implement BridgeConnection fakeit", implementation: "new() returns immediately, initialize() sets flag, no real process", type: "behavioral", status: "completed", commits: [{ hash: "03a556b", message: "feat(bridge): BridgeConnection::new() fakeit", phase: "green" }, { hash: "8287a06", message: "feat(bridge): BridgeConnection::initialize() stub", phase: "green" }], notes: [] },
      { test: "Implement LanguageServerPool fakeit methods", implementation: "completion/hover/definition/signature_help all return Ok(None)", type: "behavioral", status: "completed", commits: [{ hash: "0258b81", message: "feat(bridge): LanguageServerPool fakeit methods", phase: "green" }], notes: [] },
      { test: "Wire lsp_impl handlers to pool", implementation: "completion/hover/definition/signature_help.rs call pool methods", type: "behavioral", status: "completed", commits: [{ hash: "c995a9a", message: "feat(bridge): wire completion to pool", phase: "green" }, { hash: "d4868a7", message: "feat(bridge): wire hover/definition/signature_help", phase: "green" }], notes: [] },
      { test: "E2E test for fakeit bridge", implementation: "tests/e2e_bridge_fakeit.rs verifies no hangs", type: "behavioral", status: "completed", commits: [{ hash: "b4774b4", message: "test(e2e): add fakeit bridge tests", phase: "green" }], notes: ["377 unit tests pass", "39/40 E2E pass (1 pre-existing failure)"] },
    ] },
  ],
  // Retrospectives (recent 3) | Sprints 1-132: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    { sprint: 134, improvements: [
      { action: "Document E2E testing patterns (feature-gated visibility, testability vs API surface trade-offs) in ADR-0012 or new ADR-0013", timing: "sprint", status: "active", outcome: null },
      { action: "Create JSON-RPC framing checklist (Content-Length, \\r\\n\\r\\n separator, UTF-8 byte length, async I/O patterns)", timing: "immediate", status: "completed", outcome: "Created docs/json-rpc-framing-checklist.md with implementation guidance from Sprint 134" },
      { action: "Add performance budgets to ADR-0012 Phase 2/3 (init < 200ms, completion < 100ms, superseding < 10ms)", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 133, improvements: [
      { action: "Document two-pass (fakeit → real) strategy in ADR-0012", timing: "sprint", status: "active", outcome: null },
      { action: "Add test baseline hygiene check to Sprint Planning", timing: "sprint", status: "completed", outcome: "Sprint 134 started with clean 377 test baseline, all tests passed" },
      { action: "Establish dead code annotation convention for phased implementations", timing: "sprint", status: "completed", outcome: "Sprint 134 had no dead code warnings, convention implicitly established" },
      { action: "Create PBI for fixing test_semantic_tokens_snapshot failure", timing: "product", status: "active", outcome: null },
      { action: "Add fakeit-first checklist to acceptance criteria template", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 132, improvements: [
      { action: "Break ADR-0012 Phase 1 into multi-sprint epic", timing: "sprint", status: "completed", outcome: "PBI-178-183 created, Sprint 133 delivered PBI-178" },
      { action: "Add epic planning step to Sprint Planning", timing: "sprint", status: "completed", outcome: "Applied in Sprint 133 planning" },
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
