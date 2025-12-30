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

  // Completed PBIs: PBI-001 through PBI-095 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  product_backlog: [
    {
      id: "PBI-096",
      story: {
        role: "documentation author with Rust code blocks",
        capability:
          "see progress indicators when rust-analyzer is spawning or processing",
        benefit:
          "I understand why responses are slow and know the system is working",
      },
      acceptance_criteria: [
        {
          criterion:
            "Progress helper functions for rust-analyzer operations exist in progress.rs",
          verification:
            "Unit test: ra_progress_token, create_ra_progress_begin, create_ra_progress_end return correct ProgressParams",
        },
        {
          criterion:
            "goto_definition sends Begin progress before spawn_blocking and End progress after completion",
          verification:
            "E2E test: trigger go-to-definition on Rust code block, verify progress notification sequence",
        },
        {
          criterion:
            "hover sends Begin progress before spawn_blocking and End progress after completion",
          verification:
            "E2E test: trigger hover on Rust code block, verify progress notification sequence",
        },
        {
          criterion:
            "Progress End notification correctly indicates success or timeout/failure",
          verification:
            "Unit test: create_ra_progress_end with success=true/false returns appropriate messages",
        },
      ],
      status: "ready",
    },
  ],

  sprint: {
    number: 74,
    pbi_id: "PBI-096",
    goal:
      "Documentation authors see progress indicators during rust-analyzer operations",
    status: "review",
    subtasks: [
      {
        test: "Unit test: ra_progress_token returns correct token format 'treesitter-ls/rust-analyzer/{operation}'",
        implementation:
          "Add ra_progress_token function in progress.rs following existing progress_token pattern",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "cc6b605",
            message:
              "feat(lsp): add ra_progress_token function for rust-analyzer operations",
            phase: "green",
          },
        ],
        notes: [],
      },
      {
        test: "Unit test: create_ra_progress_begin returns ProgressParams with 'Waiting for rust-analyzer...' title",
        implementation:
          "Add create_ra_progress_begin function in progress.rs following create_progress_begin pattern",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "f7085f2",
            message:
              "feat(lsp): add create_ra_progress_begin for rust-analyzer progress",
            phase: "green",
          },
        ],
        notes: [],
      },
      {
        test: "Unit test: create_ra_progress_end with success=true returns 'rust-analyzer completed' message",
        implementation:
          "Add create_ra_progress_end function in progress.rs with success message",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "c7de187",
            message:
              "feat(lsp): add create_ra_progress_end for rust-analyzer completion",
            phase: "green",
          },
        ],
        notes: ["Combined with subtask 4 using Obvious Implementation"],
      },
      {
        test: "Unit test: create_ra_progress_end with success=false returns 'rust-analyzer timed out' message",
        implementation:
          "Update create_ra_progress_end to handle failure/timeout message",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "c7de187",
            message:
              "feat(lsp): add create_ra_progress_end for rust-analyzer completion",
            phase: "green",
          },
        ],
        notes: ["Combined with subtask 3 using Obvious Implementation"],
      },
      {
        test: "Integration test: goto_definition sends Begin progress before spawn_blocking",
        implementation:
          "Add send_notification::<Progress>(create_ra_progress_begin) before spawn_blocking in goto_definition",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "6d84254",
            message:
              "feat(lsp): add progress notifications for rust-analyzer operations",
            phase: "green",
          },
        ],
        notes: [
          "Combined with subtasks 6-8 using Obvious Implementation",
          "Follow pattern in lsp_impl.rs lines 500-502 for auto-install progress",
        ],
      },
      {
        test: "Integration test: goto_definition sends End progress after spawn_blocking completes",
        implementation:
          "Add send_notification::<Progress>(create_ra_progress_end) after spawn_blocking result handling",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "6d84254",
            message:
              "feat(lsp): add progress notifications for rust-analyzer operations",
            phase: "green",
          },
        ],
        notes: [
          "Combined with subtasks 5,7,8 using Obvious Implementation",
          "Pass success=true if definition found, success=false if timeout/None",
        ],
      },
      {
        test: "Integration test: hover sends Begin progress before spawn_blocking",
        implementation:
          "Add send_notification::<Progress>(create_ra_progress_begin) before spawn_blocking in hover",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "6d84254",
            message:
              "feat(lsp): add progress notifications for rust-analyzer operations",
            phase: "green",
          },
        ],
        notes: [
          "Combined with subtasks 5,6,8 using Obvious Implementation",
          "Same pattern as goto_definition",
        ],
      },
      {
        test: "Integration test: hover sends End progress after spawn_blocking completes",
        implementation:
          "Add send_notification::<Progress>(create_ra_progress_end) after spawn_blocking result handling",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "6d84254",
            message:
              "feat(lsp): add progress notifications for rust-analyzer operations",
            phase: "green",
          },
        ],
        notes: [
          "Combined with subtasks 5-7 using Obvious Implementation",
          "Pass success=true if hover found, success=false if timeout/None",
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

  // Historical sprints (recent 2) | Sprint 1-71: git log -- scrum.yaml, scrum.ts
  completed: [
    {
      number: 73,
      pbi_id: "PBI-095",
      goal:
        "Documentation authors get responsive go-to-definition even when rust-analyzer is slow",
      status: "done",
      subtasks: [],
    },
    {
      number: 72,
      pbi_id: "PBI-094",
      goal:
        "Documentation authors can continue using go-to-definition after rust-analyzer crashes",
      status: "done",
      subtasks: [],
    },
  ],

  // Recent 2 retrospectives | Sprint 1-71: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    {
      sprint: 73,
      improvements: [
        {
          action:
            "Document poll-based timeout pattern for blocking readers in CLAUDE.md",
          timing: "sprint",
          status: "active",
          outcome: null,
        },
        {
          action:
            "Consider serializing rust-analyzer tests to avoid parallel spawn race conditions",
          timing: "sprint",
          status: "active",
          outcome: null,
        },
      ],
    },
    {
      sprint: 72,
      improvements: [
        {
          action: "Add E2E test for go-to-definition after rust-analyzer crash",
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
