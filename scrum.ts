// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "developer using tree-sitter-ls with multiple embedded languages",
  "editor plugin author integrating tree-sitter-ls",
] as const satisfies readonly string[]; // Must have at least one role. Avoid generic roles like "user" or "admin". Remove obsolete roles freely.

const scrum: ScrumDashboard = {
  product_goal: {
    statement: "Pull-first diagnostic forwarding for embedded language regions (ADR-0020 Phase 1)",
    success_metrics: [
      { metric: "textDocument/diagnostic handler exists", target: "Handler receives DocumentDiagnosticParams and returns DocumentDiagnosticReport" },
      { metric: "Single-region diagnostic bridging works", target: "Request forwarded to downstream LS for first virtual document, response transformed to host coordinates" },
      { metric: "Multi-region diagnostic aggregation works", target: "Fan-out queries to all injection regions, diagnostics aggregated and position-transformed" },
      { metric: "E2E test coverage", target: "E2E test exists for diagnostic bridging with lua-language-server" },
    ],
  },
  product_backlog: [
    {
      id: "pbi-diagnostic-single-region",
      story: {
        role: "developer using tree-sitter-ls with multiple embedded languages",
        capability: "receive diagnostics from the first injection region in my document",
        benefit: "I can see errors in embedded code without leaving the host document",
      },
      acceptance_criteria: [
        { criterion: "textDocument/diagnostic handler advertises capability", verification: "ServerCapabilities includes diagnosticProvider" },
        { criterion: "Handler forwards request to downstream LS for first virtual document", verification: "Unit test verifies request transformation" },
        { criterion: "Diagnostic positions transformed from virtual to host coordinates", verification: "Unit test verifies position mapping" },
        { criterion: "E2E test verifies diagnostic bridging", verification: "tests/e2e_lsp_lua_diagnostic.rs exists and passes" },
      ],
      status: "done",
    },
    {
      id: "pbi-diagnostic-multi-region",
      story: {
        role: "developer using tree-sitter-ls with multiple embedded languages",
        capability: "receive diagnostics from all injection regions aggregated into a single response",
        benefit: "I can see all errors across all embedded code blocks in one diagnostic report",
      },
      acceptance_criteria: [
        { criterion: "Handler queries all injection regions in parallel", verification: "Implementation uses fan-out pattern" },
        { criterion: "Diagnostics from all regions are aggregated", verification: "Response includes diagnostics from multiple regions" },
        { criterion: "Each diagnostic's position is correctly transformed", verification: "Unit tests verify per-region position transformation" },
        { criterion: "Timeout handling for slow downstream servers", verification: "Partial results returned on timeout" },
      ],
      status: "ready",
    },
  ],
  sprint: {
    number: 17,
    pbi_id: "pbi-diagnostic-multi-region",
    goal: "Implement multi-region diagnostic aggregation with fan-out parallel queries and timeout handling",
    status: "planning",
    subtasks: [
      {
        test: "Test send_diagnostic_requests_parallel sends requests to all regions concurrently",
        implementation: "Add send_diagnostic_requests_parallel that uses futures::join_all for parallel fan-out",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["ADR-0020: Fan-out queries to downstream servers for all injection regions"],
      },
      {
        test: "Test diagnostic_impl iterates all regions and aggregates diagnostics",
        implementation: "Change from all_regions[0] to iterate all_regions, collect Vec<Diagnostic>, merge into single response",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Transform each region's positions independently using region_start_line"],
      },
      {
        test: "Test timeout handling returns partial results when some servers are slow",
        implementation: "Use tokio::time::timeout per-request (5s), continue aggregation on timeout, log warning",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["ADR-0020: Return partial results if some servers timeout"],
      },
      {
        test: "E2E test verifies multi-region aggregation",
        implementation: "Add test with multiple Lua code blocks to e2e_lsp_lua_diagnostic.rs",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Test document with 2+ Lua injection regions returns aggregated diagnostics"],
      },
    ],
  },
  completed: [
    // Sprint 16: 5 subtasks, diagnostic capability + request/response transformation + E2E test
    { number: 16, pbi_id: "pbi-diagnostic-single-region", goal: "Implement textDocument/diagnostic handler for first virtual document with position transformation", status: "done", subtasks: [] },
    // Sprint 15: 3 phases, 6 subtasks, key commits: a874a7d9, 52d7c3d0, a0c16e97
    { number: 15, pbi_id: "pbi-cancellation-forwarding", goal: "Implement $/cancelRequest notification forwarding to downstream servers while preserving pending request entries", status: "done", subtasks: [] },
    // Sprint 14: 4 phases, 6 acceptance criteria, key commits: eefa609a, 67b9db3d, b2721d65, cfe5cd33
    { number: 14, pbi_id: "pbi-liveness-timeout", goal: "Implement liveness timeout to detect and recover from hung downstream servers", status: "done", subtasks: [] },
    // Sprint 13: 5 phases, 7 subtasks, key commits: b4f667bb, c0e58e62, 7e88b266, 4155548f, 23131874, aaa2954b, b76d5878
    { number: 13, pbi_id: "pbi-global-shutdown-timeout", goal: "Implement global shutdown timeout with configurable ceiling and force-kill fallback", status: "done", subtasks: [] },
    { number: 12, pbi_id: "pbi-lsp-shutdown", goal: "Implement connection lifecycle with graceful LSP shutdown handshake", status: "done", subtasks: [] },
  ],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
      { name: "E2E test exists for bridged features (test infrastructure even if downstream LS returns no data)", run: "verify tests/e2e_lsp_lua_*.rs exists for feature" },
    ],
  },
  retrospectives: [
    { sprint: 16, improvements: [
      { action: "Bridge method classification documented: whole-document methods (diagnostic, document_symbol) vs position-specific methods (hover, definition) - helps choose correct request pattern", timing: "immediate", status: "completed", outcome: "Pattern clearly documented in subtask notes; diagnostic follows document_symbol pattern without position." },
      { action: "Sprint 15 retrospective action validated: Clippy collapsible_if caught at review phase; 'make check' after each green phase would catch earlier", timing: "sprint", status: "active", outcome: null },
      { action: "First-region-only scoping enables incremental delivery: Sprint 16 delivers value while Sprint 17 adds aggregation", timing: "immediate", status: "completed", outcome: "Clean separation allowed focused implementation; all_regions[0] pattern makes Sprint 17 extension clear." },
    ] },
    { sprint: 15, improvements: [
      { action: "Document BridgeCoordinator delegation pattern: handlers access pool directly, delegating methods added to coordinator are unused - clarify API design expectations upfront", timing: "immediate", status: "completed", outcome: "Added API Design Pattern section to coordinator.rs documenting two access patterns: direct pool access (preferred) vs delegating methods (convenience). Clarifies YAGNI principle for new features." },
      { action: "Run 'make check' after each green phase, not just at review - catches Clippy issues (missing Default impl, nested if) earlier", timing: "sprint", status: "active", outcome: null },
      { action: "Notification forwarding pattern validated: CancelMap design cleanly separates upstream/downstream ID mapping; similar pattern reusable for other notifications requiring ID translation", timing: "immediate", status: "completed", outcome: "CancelMap with DashMap provided clean thread-safe upstream->downstream mapping; retain() pattern acceptable for typical request counts." },
    ] },
    { sprint: 14, improvements: [
      { action: "Pattern validated: LivenessTimeout reused GlobalShutdownTimeout newtype pattern; BoundedDuration extraction viable for future", timing: "immediate", status: "completed", outcome: "Sprint 14 reused validation pattern from Sprint 13 without issues." },
      { action: "Timer infrastructure in reader task scales well - select! multiplexing isolates liveness from connection lifecycle", timing: "immediate", status: "completed", outcome: "Clean separation of concerns validated." },
      { action: "Test gap: No integration test for timer reset on stdout activity", timing: "product", status: "active", outcome: null },
      { action: "Documentation: Add Sprint 14 as phased implementation case study to ADR-0013", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 13, improvements: [
      { action: "ADR-first approach: Document patterns BEFORE implementation, tests verify ADR compliance", timing: "sprint", status: "completed", outcome: "Sprint 14 followed ADR-0014/ADR-0018 precisely." },
      { action: "Document phased implementation pattern (Foundation -> Core -> Robustness)", timing: "product", status: "active", outcome: null },
      { action: "Consider BoundedDuration(min, max) extraction for timeout newtypes", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 12, improvements: [
      { action: "Document enum variant call site update requirements when adding variants", timing: "sprint", status: "active", outcome: null },
    ] },
  ],
};

// Type Definitions (DO NOT MODIFY) =============================================
// PBI lifecycle: draft (idea) -> refining (gathering info) -> ready (can start) -> done
type PBIStatus = "draft" | "refining" | "ready" | "done";

// Sprint lifecycle
type SprintStatus =
  | "planning"
  | "in_progress"
  | "review"
  | "done"
  | "cancelled";

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

interface SuccessMetric {
  metric: string;
  target: string;
}
interface ProductGoal {
  statement: string;
  success_metrics: SuccessMetric[];
}
interface AcceptanceCriterion {
  criterion: string;
  verification: string;
}
interface UserStory {
  role: (typeof userStoryRoles)[number];
  capability: string;
  benefit: string;
}
interface PBI {
  id: string;
  story: UserStory;
  acceptance_criteria: AcceptanceCriterion[];
  status: PBIStatus;
  refinement_notes?: string[];
}
interface Commit {
  hash: string;
  message: string;
  phase: CommitPhase;
}
interface Subtask {
  test: string;
  implementation: string;
  type: SubtaskType;
  status: SubtaskStatus;
  commits: Commit[];
  notes: string[];
}
interface Sprint {
  number: number;
  pbi_id: string;
  goal: string;
  status: SprintStatus;
  subtasks: Subtask[];
}
interface DoDCheck {
  name: string;
  run: string;
}
interface DefinitionOfDone {
  checks: DoDCheck[];
}
interface Improvement {
  action: string;
  timing: ImprovementTiming;
  status: ImprovementStatus;
  outcome: string | null;
}
interface Retrospective {
  sprint: number;
  improvements: Improvement[];
}
interface ScrumDashboard {
  product_goal: ProductGoal;
  product_backlog: PBI[];
  sprint: Sprint | null;
  definition_of_done: DefinitionOfDone;
  completed: Sprint[];
  retrospectives: Retrospective[];
}

// JSON output (deno run scrum.ts | jq for queries)
console.log(JSON.stringify(scrum, null, 2));
