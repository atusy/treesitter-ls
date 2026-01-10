// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Rustacean editing markdown",
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

  // Completed PBIs: PBI-001-140 (Sprint 1-113), PBI-155-161 (124-130), PBI-178-180a (133-135), PBI-184 (136), PBI-181 (137), PBI-185 (138), PBI-187 (139), PBI-180b (140), PBI-190 (141), PBI-191 (142), PBI-192 (143)
  // Deferred: PBI-091, PBI-107 | Removed: PBI-163-177 | Superseded: PBI-183 | Cancelled: Sprint 139 PBI-180b attempt
  // Sprint 139-143: All sprints DONE (Sprint 143: unit tests + code quality PASSED, E2E test infrastructure issue documented)
  // Walking Skeleton PBIs: PBI-301 through PBI-304 (Draft)
  product_backlog: [
    // ============================================================
    // Walking Skeleton: Incremental Bridge Feature Development
    // Each PBI delivers independently testable, demonstrable value
    // ============================================================

    // --- PBI-301: Basic Bridge Attachment (Minimal Viable Slice) ---
    {
      id: "PBI-301",
      story: {
        role: "Rustacean editing markdown",
        capability:
          "notice rust-analyzer is attached to Rust code block in markdown",
        benefit:
          "I can expect rust-analyzer features",
      },
      acceptance_criteria: [], // Draft - AC to be added during refinement
      status: "draft",
      refinement_notes: [
        "Thinnest possible slice - just spawn the server and confirm it's running",
        "No initialization window required (assume notifications/requests start after initialized)",
        "No shutdown required (assume child process rust-analyzer is killed by termination of parent process treesitter-ls)",
        "Assume single language server support. No need of language server pool yet",
        "Editor shows 'rust-analyzer spawned'",
        "No useful features yet (no hover, no completions, ...)",
        "Proves the architecture works",
      ],
    },

    // --- PBI-302: Hover Feature ---
    {
      id: "PBI-302",
      story: {
        role: "Rustacean editing markdown",
        capability:
          "hover from rust-analyzer in Rust code block in markdown",
        benefit:
          "I understand the details of objects",
      },
      acceptance_criteria: [], // Draft - AC to be added during refinement
      status: "draft",
      refinement_notes: [
        "First actual LSP feature on top of the basic bridge",
        "Proves request/response flow works",
        "Builds on PBI-301 (basic bridge attachment)",
      ],
    },

    // --- PBI-303: Completions Feature ---
    {
      id: "PBI-303",
      story: {
        role: "Rustacean editing markdown",
        capability:
          "completions from rust-analyzer in Rust code block in markdown",
        benefit:
          "I can write faster",
      },
      acceptance_criteria: [], // Draft - AC to be added during refinement
      status: "draft",
      refinement_notes: [
        "Second LSP feature",
        "Proves notifications work",
        "Builds on PBI-301 and PBI-302",
      ],
    },

    // --- PBI-304: Non-Blocking Initialization ---
    {
      id: "PBI-304",
      story: {
        role: "Rustacean editing markdown",
        capability:
          "bridge server initialization never blocks treesitter-ls functionality",
        benefit:
          "I can edit regardless of rust-analyzer state",
      },
      acceptance_criteria: [], // Draft - AC to be added during refinement
      status: "draft",
      refinement_notes: [
        "Add initialization window",
        "treesitter-ls remains responsive during rust-analyzer startup",
        "Requests during initialization return appropriate errors (not hang)",
        "Improves user experience",
        "Builds on PBI-301, PBI-302, PBI-303",
      ],
    },
  ],
  sprint: null,
  completed: [],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
  // Retrospectives (recent 4) | Sprints 1-139: git log -- scrum.yaml, scrum.ts
  retrospectives: [],
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
