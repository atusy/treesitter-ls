// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "developer using tree-sitter-ls with multiple embedded languages",
  "editor plugin author integrating tree-sitter-ls",
] as const satisfies readonly string[]; // Must have at least one role. Avoid generic roles like "user" or "admin". Remove obsolete roles freely.

const scrum: ScrumDashboard = {
  product_goal: {
    statement: "Reliable connection lifecycle for embedded language servers",
    success_metrics: [
      { metric: "Connection state machine completeness", target: "ConnectionState includes Initializing, Ready, Failed, Closing, Closed with all Phase 1 transitions implemented" },
      { metric: "LSP shutdown handshake compliance", target: "Graceful shutdown sends shutdown request, waits for response, sends exit notification per LSP spec" },
      { metric: "Timeout hierarchy implementation", target: "Init timeout (30-60s), Liveness timeout (30-120s), Global shutdown timeout (5-15s) with correct precedence" },
      { metric: "Cancellation forwarding", target: "$/cancelRequest notifications forwarded to downstream servers while keeping pending request entries" },
    ],
  },
  product_backlog: [
    // All current PBIs are done - Product Goal achieved!
    // Done PBIs: pbi-liveness-timeout (Sprint 14), pbi-cancellation-forwarding (Sprint 15)
  ],
  sprint: null,
  completed: [
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
