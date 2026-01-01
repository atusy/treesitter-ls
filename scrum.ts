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

  // Completed PBIs: PBI-001 through PBI-135 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Deferred - infrastructure already implemented, needs wiring (low priority)
  // PBI-107 (remove WorkspaceType): Deferred - rust-analyzer linkedProjects too slow
  product_backlog: [
    {
      id: "PBI-136",
      story: {
        role: "developer editing Lua files",
        capability: "have the legacy synchronous bridge pool removed",
        benefit: "codebase is simpler with only one connection management pattern",
      },
      acceptance_criteria: [
        { criterion: "LanguageServerPool removed from TreeSitterLs", verification: "language_server_pool field removed; only async_language_server_pool remains" },
        { criterion: "Legacy pool module can be removed", verification: "pool.rs, connection.rs only used for async pool initialization; sync methods removed or deprecated" },
        { criterion: "All tests pass without legacy pool", verification: "make test && make check && make test_nvim all pass" },
      ],
      status: "draft",
    },
  ],

  sprint: null,

  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },

  // Historical sprints (recent 2) | Sprint 1-111: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 113, pbi_id: "PBI-135", goal: "Migrate all remaining bridge handlers to async pool pattern", status: "done", subtasks: [] },
    { number: 112, pbi_id: "PBI-134", goal: "Implement async request/response queue pattern for bridge connections", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-110: modular refactoring pattern, E2E indexing waits, root cause investigation
  retrospectives: [
    {
      sprint: 113,
      improvements: [
        { action: "publishDiagnostics-based indexing detection is robust - polling for the notification with timeout is more reliable than fixed 500ms sleep", timing: "immediate", status: "completed", outcome: "did_open_and_wait() polls is_ready() with 50ms interval and 10s timeout; works reliably across handler types" },
        { action: "Consider extracting common bridge handler boilerplate into shared helper - 17 handlers repeat same pattern (get doc, check language, get injections, translate positions, get connection, etc.)", timing: "sprint", status: "active", outcome: null },
        { action: "User suggests exploring wrapper pattern alternatives on separate branch for potentially cleaner/more performant design", timing: "product", status: "active", outcome: null },
      ],
    },
    {
      sprint: 112,
      improvements: [
        { action: "AsyncConnectionWithInfo wrapper pattern works well - stores per-connection state (virtual_file_path, version) alongside the connection", timing: "immediate", status: "completed", outcome: "Cleaner than modifying AsyncBridgeConnection directly; separation of concerns" },
        { action: "Incremental migration is valid - migrating high-frequency handlers first provides immediate value while deferring rest to next sprint", timing: "immediate", status: "completed", outcome: "4 handlers migrated in PBI-134; remaining 17 handlers completed in PBI-135" },
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
