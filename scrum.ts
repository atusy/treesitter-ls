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

  // Completed: PBI-001-192 (Sprint 1-143), PBI-301 (144), PBI-302 (145) | Deferred: PBI-091, PBI-107
  // Walking Skeleton: PBI-303, PBI-304 (Ready)
  product_backlog: [
    // --- PBI-303: Completions Feature ---
    {
      id: "PBI-303",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "completions from lua-language-server in Lua code block in markdown",
        benefit:
          "I can write Lua code faster with autocomplete",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given a Lua code block, when I type a partial identifier, then I see completion suggestions from lua-language-server",
          verification:
            "E2E test: typing 'pri' in Lua block shows 'print' completion",
        },
        {
          criterion:
            "Given a Lua code block with a table variable, when I type the table name followed by dot, then I see member completions",
          verification:
            "E2E test: typing 'string.' shows string library methods",
        },
        {
          criterion:
            "Given document changes in markdown, when textDocument/didOpen is sent, then virtual Lua document is synced to lua-language-server",
          verification:
            "Unit test: verify didOpen notification sent with correct virtual document content",
        },
        {
          criterion:
            "Given document changes in markdown, when textDocument/didChange is sent, then virtual Lua document is updated in lua-language-server",
          verification:
            "Unit test: verify didChange notification sent with correct incremental changes",
        },
        {
          criterion:
            "Given completion items from lua-language-server, when items are returned to client, then they are relevant to the Lua context",
          verification:
            "E2E test: completions include Lua-specific items (local, function, table, etc.)",
        },
      ],
      status: "ready",
      refinement_notes: ["Second LSP feature; proves notifications (didOpen/didChange); requires doc sync; builds on PBI-301/302"],
    },

    // --- PBI-304: Non-Blocking Initialization ---
    {
      id: "PBI-304",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "bridge server initialization never blocks treesitter-ls functionality",
        benefit:
          "I can edit markdown regardless of lua-language-server state",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given lua-language-server is starting up, when treesitter-ls receives any request, then treesitter-ls responds without blocking",
          verification:
            "Integration test: send requests during lua-ls startup, verify response time < 100ms",
        },
        {
          criterion:
            "Given lua-language-server is initializing, when hover/completion request is sent for Lua block, then an appropriate error response is returned (not timeout)",
          verification:
            "Unit test: verify error response with message indicating bridge not ready",
        },
        {
          criterion:
            "Given lua-language-server initialization completes, when bridge transitions to ready state, then subsequent requests are handled normally",
          verification:
            "Integration test: verify requests succeed after initialization completes",
        },
        {
          criterion:
            "Given user is editing markdown, when lua-language-server is initializing, then markdown editing features (syntax highlighting, folding) continue to work",
          verification:
            "E2E test: verify treesitter-ls native features work during bridge initialization",
        },
      ],
      status: "ready",
      refinement_notes: ["ADR-0018 init window; async handling; errors not hangs during init; builds on PBI-301/302/303"],
    },
  ],
  sprint: null,
  // Sprint 145 (PBI-302): 9 subtasks, commits: 09dcfd1e, e4dbb8f8, 50d7f096, 764e64d8, a475c413, 13941068
  // Sprint 144 (PBI-301): 7 subtasks, commits: 1393ded9, a7116891, 4ff80258, d48e9557, 551917f1, 89a2e1f6, 525661d9
  completed: [],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
  // Retrospectives: Sprint 145 (perf regression, BrokenPipe, E2E robustness), Sprint 144 (bridge split done)
  retrospectives: [
    { sprint: 145, improvements: [
      { action: "Perf regression: test_incremental_tokenization (29ms vs 20ms)", timing: "product", status: "active", outcome: null },
      { action: "BrokenPipe in e2e_lsp_didchange_updates_state", timing: "product", status: "active", outcome: null },
      { action: "E2E test infrastructure robustness", timing: "product", status: "active", outcome: null },
    ]},
    { sprint: 144, improvements: [
      { action: "Bridge.rs split", timing: "sprint", status: "completed", outcome: "Split into 4 submodules" },
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
