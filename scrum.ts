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
      "Improve LSP bridge go-to-definition to be production ready (ADR-0006, 0007, 0008)",
    success_metrics: [
      {
        metric: "Connection pooling implemented",
        target: "Server connections reused across requests",
      },
      {
        metric: "Configuration system complete",
        target: "User can configure bridge servers via initializationOptions",
      },
      {
        metric: "Robustness features",
        target: "Ready detection, timeout handling, crash recovery",
      },
    ],
  },

  // Completed PBIs: PBI-001 through PBI-102 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  // PBI-098: Language-based routing - already implemented as part of PBI-097 (configurable bridge servers)
  product_backlog: [
    {
      id: "PBI-103",
      story: {
        role: "documentation author with Rust code blocks",
        capability:
          "see actual rust-analyzer progress notifications instead of synthetic 'Waiting' messages",
        benefit:
          "I know exactly what rust-analyzer is doing (Loading crates, Indexing, etc.) during operations",
      },
      acceptance_criteria: [
        {
          criterion:
            "Synthetic create_ra_progress_begin/end messages are removed from goto_definition and hover",
          verification:
            "Grep for create_ra_progress_begin shows no usage in lsp_impl.rs",
        },
        {
          criterion:
            "$/progress notifications from rust-analyzer are captured during read_response_for_id",
          verification:
            "Unit test: read_response_for_id collects $/progress notifications",
        },
        {
          criterion:
            "Captured progress notifications are returned alongside the response for forwarding",
          verification:
            "Unit test: goto_definition returns collected notifications",
        },
        {
          criterion:
            "lsp_impl forwards the captured $/progress notifications to the client",
          verification:
            "Integration test: client receives actual rust-analyzer progress tokens",
        },
      ],
      status: "done",
    },
  ],

  sprint: {
    number: 80,
    pbi_id: "PBI-103",
    goal:
      "Documentation authors see actual rust-analyzer progress notifications instead of synthetic 'Waiting' messages so they know exactly what rust-analyzer is doing",
    status: "done",
    subtasks: [
      {
        test: "read_response_for_id returns captured notifications alongside response",
        implementation:
          "Create ResponseWithNotifications struct containing (Option<Value>, Vec<Value>) for response and captured notifications; modify read_response_for_id to collect $/progress method notifications instead of skipping them; return the new struct type",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "7e23987",
            message:
              "feat(progress): add ResponseWithNotifications and read_response_for_id_with_notifications",
            phase: "green",
          },
        ],
        notes: [
          "Current implementation in redirection.rs line 378-413 skips all non-matching messages",
          "Need to capture notifications where method == '$/progress'",
          "Other notifications should still be skipped",
        ],
      },
      {
        test: "goto_definition returns captured progress notifications alongside result",
        implementation:
          "Modify goto_definition() in redirection.rs to use the new ResponseWithNotifications; update return type to include Vec<Value> for captured notifications",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "38a6fc0",
            message:
              "feat(progress): add goto_definition_with_notifications method",
            phase: "green",
          },
        ],
        notes: [
          "Current signature: goto_definition(&mut self, _uri: &str, position: Position) -> Option<GotoDefinitionResponse>",
          "Need to return (Option<GotoDefinitionResponse>, Vec<Value>) or a similar struct",
        ],
      },
      {
        test: "hover returns captured progress notifications alongside result",
        implementation:
          "Modify hover() in redirection.rs to use the new ResponseWithNotifications; update return type to include Vec<Value> for captured notifications",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "6f57cf9",
            message: "feat(progress): add hover_with_notifications method",
            phase: "green",
          },
        ],
        notes: [
          "Current signature: hover(&mut self, _uri: &str, position: Position) -> Option<Hover>",
          "Same pattern as goto_definition change",
        ],
      },
      {
        test: "lsp_impl goto_definition forwards captured notifications to client",
        implementation:
          "Update lsp_impl.rs goto_definition to handle the new return type; for each captured notification, parse as ProgressParams and forward via client.send_notification::<Progress>; remove synthetic create_ra_progress_begin/end calls",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "021ecd7",
            message:
              "feat(progress): forward captured progress notifications in goto_definition and hover",
            phase: "green",
          },
        ],
        notes: [
          "Current code in lsp_impl.rs lines 1960-1997 uses create_ra_progress_begin/end",
          "Remove those calls and replace with forwarding of captured notifications",
          "serde_json::from_value to parse notification params as ProgressParams",
        ],
      },
      {
        test: "lsp_impl hover forwards captured notifications to client",
        implementation:
          "Update lsp_impl.rs hover to handle the new return type; for each captured notification, parse as ProgressParams and forward via client.send_notification::<Progress>; remove synthetic create_ra_progress_begin/end calls",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "021ecd7",
            message:
              "feat(progress): forward captured progress notifications in goto_definition and hover",
            phase: "green",
          },
        ],
        notes: [
          "Current code in lsp_impl.rs lines 2204-2241 uses create_ra_progress_begin/end",
          "Same pattern as goto_definition",
        ],
      },
      {
        test: "ra_progress_token and create_ra_progress_begin/end can be removed from progress.rs",
        implementation:
          "Remove ra_progress_token, create_ra_progress_begin, create_ra_progress_end from progress.rs since they are no longer used; update imports in lsp_impl.rs",
        type: "structural",
        status: "completed",
        commits: [
          {
            hash: "3a3fbc7",
            message:
              "refactor(progress): remove unused ra_progress_* functions",
            phase: "refactoring",
          },
        ],
        notes: [
          "This is cleanup after AC1 is complete",
          "Verify grep shows no remaining usages before removal",
          "Keep the parser installation progress functions (progress_token, create_progress_begin, create_progress_end)",
        ],
      },
    ],
  },

  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },

  // Historical sprints (recent 2) | Sprint 1-77: git log -- scrum.yaml, scrum.ts
  completed: [
    {
      number: 79,
      pbi_id: "PBI-102",
      goal:
        "Documentation authors have bridge server connections pre-warmed when opening a document so their first go-to-definition request is fast",
      status: "done",
      subtasks: [],
    },
    {
      number: 78,
      pbi_id: "PBI-101",
      goal:
        "Documentation authors have spawn() use workspace_type configuration so the workspace type feature works end-to-end",
      status: "done",
      subtasks: [],
    },
  ],

  // Recent 2 retrospectives | Sprint 1-77: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    {
      sprint: 79,
      improvements: [
        {
          action:
            "Arc<DashMap> pattern enables clean async sharing - consider documenting this pattern for future concurrent features",
          timing: "immediate",
          status: "completed",
          outcome:
            "Pattern documented in CLAUDE.md DashMap Lock Pattern section (Sprint 71). Pattern reused successfully here.",
        },
        {
          action:
            "Add E2E test verifying eager spawn reduces first goto-definition latency in real Markdown file",
          timing: "product",
          status: "active",
          outcome: null,
        },
      ],
    },
    {
      sprint: 78,
      improvements: [
        {
          action:
            "Add E2E test with pyright to verify Generic workspace works end-to-end with a real language server",
          timing: "product",
          status: "active",
          outcome: null,
        },
        {
          action:
            "Consider consolidating spawn_rust_analyzer() into spawn() with Cargo workspace_type to reduce code duplication",
          timing: "product",
          status: "active",
          outcome: null,
        },
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
