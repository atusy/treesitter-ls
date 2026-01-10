// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Lua developer editing markdown",
] as const satisfies readonly string[]; // Must have at least one role. Avoid generic roles like "user" or "admin". Remove obsolete roles freely.

const scrum: ScrumDashboard = {
  product_goal: {
    statement:
      "Implement LSP bridge to support essential language server features indirectly through bridging (ADR-0013, 0014, 0015, 0016, 0017, 0018)",
    success_metrics: [
      {
        metric: "ADR alignment",
        target:
          "Must align with Phase 1 of ADR-0013, 0014, 0015, 0016, 0017, 0018 in @docs/adr",
      },
      {
        metric: "Bridge coverage",
        target:
          "Support completion, signatureHelp, codeAction, definition, hover",
      },
      {
        metric: "Modular architecture",
        target:
          "Bridge module organized with text_document/ subdirectory matching lsp_impl structure",
      },
      {
        metric: "E2E test coverage",
        target: "Each bridged feature has E2E test verifying end-to-end flow",
      },
    ],
  },

  // Completed: PBI-001-304 (1-147) | Walking Skeleton complete! | Deferred: PBI-091, PBI-107
  // Remaining: PBI-305 (lua-ls config investigation)
  product_backlog: [

    // --- PBI-305: lua-language-server Workspace Configuration ---
    {
      id: "PBI-305",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "have lua-language-server return completion items in virtual documents",
        benefit:
          "I can get actual autocomplete suggestions instead of null responses",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given a completion request in Lua code block, when lua-ls is initialized with proper workspace configuration, then lua-ls returns completion items",
          verification:
            "E2E test: typing 'pri' in Lua block receives non-null completion response from lua-ls",
        },
        {
          criterion:
            "Given virtual document URI, when lua-ls indexes the document, then lua-ls recognizes the virtual file scheme",
          verification:
            "Test with file:// scheme variations or document materialization (ADR-0007)",
        },
        {
          criterion:
            "Given lua-ls initialization, when rootUri or workspaceFolder is provided, then lua-ls initializes workspace correctly",
          verification:
            "Verify lua-ls telemetry/logs show successful workspace indexing",
        },
      ],
      status: "ready",
      refinement_notes: ["Blocking issue from PBI-303 Sprint 146; investigate lua-ls config requirements; may require ADR-0007 document materialization"],
    },
  ],
  sprint: null,
  // Sprint 147 (PBI-304): 8 subtasks, commits: 34c5c4c7, 9d563274, eccf6c09, b5ed4458, efbdabf9
  // Sprint 146 (PBI-303): 8/10 subtasks, commits: 55abb5e0..b5588476 (E2E blocked by lua-ls)
  // Sprint 145 (PBI-302): 9 subtasks | Sprint 144 (PBI-301): 7 subtasks
  completed: [],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
  // Retrospectives: 147 (E2E success, state integration), 146 (E2E early, distribute tests), 145 (BrokenPipe)
  retrospectives: [
    { sprint: 147, improvements: [
      { action: "Mark Sprint 146 'E2E early' action as completed", timing: "immediate", status: "completed", outcome: "E2E tests ran early in Sprint 147; non-blocking behavior verified" },
      { action: "Integrate set_ready()/set_failed() state transitions in BridgeManager initialization flow", timing: "sprint", status: "active", outcome: null },
      { action: "Investigate BrokenPipe E2E issue (unresolved for 2 sprints)", timing: "product", status: "active", outcome: null },
    ]},
    { sprint: 146, improvements: [
      { action: "Run E2E smoke test early when involving external LS", timing: "sprint", status: "completed", outcome: "Applied in Sprint 147; E2E tests ran early and passed" },
      { action: "Distribute E2E tests across sprint", timing: "sprint", status: "active", outcome: null },
    ]},
    { sprint: 145, improvements: [
      { action: "Perf regression + BrokenPipe E2E issues", timing: "product", status: "active", outcome: null },
    ]},
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
