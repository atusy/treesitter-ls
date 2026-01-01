// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Rustacean editing Markdown",
  "developer editing Lua files",
  "documentation author with Rust code blocks",
] as const satisfies readonly string[]; // Must have at least one role. Avoid generic roles like "user" or "admin". Remove obsolete roles freely.

const scrum: ScrumDashboard = {
  product_goal: {
    statement:
      "Expand LSP bridge to support most language server features indirectly through bridging (ADR-0006, 0007, 0008)",
    success_metrics: [
      {
        metric: "Bridge coverage",
        target:
          "Support completion, signatureHelp, references, rename, codeAction, formatting, typeDefinition, implementation, documentHighlight, declaration, inlayHint, callHierarchy, typeHierarchy, documentLink, foldingRange",
      },
      {
        metric: "Modular architecture",
        target: "Bridge module organized with text_document/ subdirectory matching lsp_impl structure",
      },
      {
        metric: "E2E test coverage",
        target: "Each bridged feature has E2E test verifying end-to-end flow",
      },
    ],
  },

  // Completed PBIs: PBI-001 through PBI-140 (Sprint 1-113) | History: git log -- scrum.yaml, scrum.ts
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  // ADR-0009 Implementation: PBI-144 (Sprint 114), PBI-145 (Sprint 115), PBI-148 (Sprint 116), PBI-146 (Sprint 117), PBI-149 (Sprint 118), PBI-142 (Sprint 120), PBI-143 (Sprint 121)
  // Rejected: PBI-147 (wait for indexing) - replaced by PBI-149
  product_backlog: [],

  sprint: null,

  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },

  // Historical sprints (recent 2) | Sprint 1-120: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 121, pbi_id: "PBI-143", goal: "Implement fully async signatureHelp for Rust code blocks in Markdown, completing ADR-0009 async migration for high-frequency LSP methods", status: "done", subtasks: [] },
    { number: 120, pbi_id: "PBI-142", goal: "Implement fully async completion with TokioAsyncLanguageServerPool", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-116: modular refactoring pattern, E2E indexing waits, vertical slice validation, RAII cleanup
  retrospectives: [
    { sprint: 121, improvements: [
      { action: "ADR-0009 Phase 3 COMPLETE: Three high-frequency LSP methods (hover, completion, signatureHelp) now fully async - MAJOR MILESTONE achieved across 3 sprints (118, 120-121)", timing: "sprint", status: "active", outcome: null },
      { action: "Pattern emerged: All 3 async methods share identical structure (get_connection -> sync_document -> send_request -> parse_response) - Lines 415-560 in tokio_async_pool.rs show duplication", timing: "product", status: "active", outcome: null },
      { action: "Future refactoring opportunity: Extract generic async_request<Req, Res>(method, params_builder) to eliminate 150+ lines of duplication while preserving type safety", timing: "product", status: "active", outcome: null },
      { action: "Multi-sprint pattern reuse successful: ServerState tracking lesson from Sprint 118 prevented regression in Sprints 120-121 - evidence that retrospective improvements carry forward effectively", timing: "sprint", status: "active", outcome: null },
      { action: "Sprint velocity consistent: Each async method completed in single sprint (PBI-149/118 hover, PBI-142/120 completion, PBI-143/121 signatureHelp) - 30s timeout standard, no E2E changes needed", timing: "sprint", status: "active", outcome: null },
    ] },
    { sprint: 120, improvements: [
      { action: "Pattern established: hover/completion share identical structure - consider extracting common async request handler", timing: "sprint", status: "active", outcome: null },
      { action: "E2E timeout pattern emerging (15s -> 90s for async indexing) - document timeout rationale in test files", timing: "immediate", status: "active", outcome: null },
      { action: "Sprint 117 lesson (document version tracking) successfully prevented regression - continue applying lessons from previous retrospectives", timing: "sprint", status: "active", outcome: null },
      { action: "Two async methods (hover, completion) follow same pattern - opportunity for DRY refactoring with generic request handler", timing: "product", status: "active", outcome: null },
    ] },
  ],
};

// ============================================================
// Type Definitions (DO NOT MODIFY - request human review for schema changes)
// ============================================================

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
