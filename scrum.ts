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

  // Completed PBIs: PBI-001 through PBI-132 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Deferred - infrastructure already implemented, needs wiring (low priority)
  // PBI-107 (remove WorkspaceType): Deferred - rust-analyzer linkedProjects too slow
  product_backlog: [
    {
      id: "PBI-132",
      story: {
        role: "developer editing Lua files",
        capability: "have treesitter-ls remain responsive when bridged language servers are slow or unresponsive",
        benefit: "the LSP does not hang indefinitely when rust-analyzer or other bridged servers fail to respond",
      },
      acceptance_criteria: [
        { criterion: "Bridge I/O operations have configurable timeout", verification: "read_response_for_id_with_notifications accepts timeout parameter; defaults to 30 seconds" },
        { criterion: "Timeout triggers graceful error handling", verification: "When timeout expires, function returns None response with empty notifications; no infinite loop" },
        { criterion: "All bridge request methods use timeout", verification: "22+ *_with_notifications methods pass DEFAULT_TIMEOUT to read function" },
        { criterion: "Unit test verifies timeout behavior", verification: "Test read_response_for_id_with_notifications returns None after timeout" },
      ],
      status: "done",
    },
    {
      id: "PBI-133",
      story: {
        role: "developer editing Lua files",
        capability: "have DashMap operations in DocumentStore avoid deadlocks",
        benefit: "the LSP does not freeze when concurrent document updates occur",
      },
      acceptance_criteria: [
        { criterion: "DashMap read locks released before write operations", verification: "update_document extracts data from read lock, drops lock, then performs insert" },
        { criterion: "All DashMap usages follow safe pattern", verification: "Code review confirms no read lock held while calling methods needing write access" },
      ],
      status: "draft",
    },
  ],

  sprint: null, // Sprint 109 (PBI-132) completed - bridge I/O timeout

  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },

  // Historical sprints (recent 2) | Sprint 1-108: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 109, pbi_id: "PBI-132", goal: "Add timeout mechanism to bridge I/O operations", status: "done", subtasks: [] },
    { number: 108, pbi_id: "PBI-131", goal: "Add textDocument/foldingRange bridge support", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-107: modular refactoring pattern, E2E indexing waits
  retrospectives: [
    {
      sprint: 109,
      improvements: [
        { action: "Blocking I/O timeout checks happen between read operations, not during - for truly responsive timeout would need async I/O", timing: "product", status: "completed", outcome: "30s timeout prevents indefinite hangs; async migration deferred" },
        { action: "PBIs should have ≤4 acceptance criteria to fit in one sprint; large PBIs should be split", timing: "immediate", status: "completed", outcome: "PBI-132 split from original 6 ACs to 4 ACs; DashMap fix moved to PBI-133" },
      ],
    },
    {
      sprint: 108,
      improvements: [
        { action: "FoldingRange uses startLine/endLine integers - translate line numbers directly", timing: "immediate", status: "completed", outcome: "Line number translation for folding ranges" },
      ],
    },
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
