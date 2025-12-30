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

  // Completed PBIs: PBI-001 through PBI-090, PBI-093, PBI-094 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  product_backlog: [
    {
      id: "PBI-094",
      story: {
        role: "documentation author with Rust code blocks",
        capability:
          "continue using go-to-definition after rust-analyzer crashes",
        benefit:
          "I don't lose productivity when the language server has issues",
      },
      acceptance_criteria: [
        {
          criterion:
            "go-to-definition works after rust-analyzer process is killed",
          verification:
            "E2E test: goto-definition, kill rust-analyzer, goto-definition again succeeds",
        },
        {
          criterion: "Server respawn happens automatically without user action",
          verification: "No manual restart required; next request triggers respawn",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-095",
      story: {
        role: "documentation author with Rust code blocks",
        capability:
          "get a response within seconds even when rust-analyzer is slow",
        benefit: "I'm not blocked waiting for a hung language server",
      },
      acceptance_criteria: [
        {
          criterion: "Request times out after configurable duration (default 5s)",
          verification: "Unit test: mock slow server, verify timeout",
        },
        {
          criterion: "Timeout returns graceful null response, not error",
          verification: "Unit test: timeout produces None, not panic/error",
        },
      ],
      status: "ready",
    },
  ],

  sprint: {
    number: 73,
    pbi_id: "PBI-095",
    goal:
      "Documentation authors get responsive go-to-definition even when rust-analyzer is slow",
    status: "in_progress",
    subtasks: [
      {
        test:
          "goto_definition with mock slow server returns None after timeout (default 5s)",
        implementation:
          "Wrap read_response_for_id in tokio::time::timeout(), return None on Elapsed",
        type: "behavioral",
        status: "green",
        commits: [],
        notes: [
          "Used poll()-based timeout on Unix instead of tokio::time::timeout for simpler integration with blocking BufReader",
        ],
      },
      {
        test:
          "goto_definition timeout is configurable via connection parameter",
        implementation:
          "Add timeout_duration field to LanguageServerConnection, use in timeout() call",
        type: "behavioral",
        status: "green",
        commits: [],
        notes: [],
      },
      {
        test: "hover request also respects timeout configuration",
        implementation:
          "Apply same timeout pattern to hover() method for consistency",
        type: "behavioral",
        status: "green",
        commits: [],
        notes: [],
      },
      {
        test: "timeout returns graceful None, not panic or error propagation",
        implementation:
          "Verify Option<T> return type handles timeout as None, no unwrap/expect",
        type: "behavioral",
        status: "green",
        commits: [],
        notes: [
          "Implementation already uses Option<T> and ? operator for graceful error handling",
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

  // Historical sprints (recent 2) | Sprint 1-70: git log -- scrum.yaml, scrum.ts
  completed: [
    {
      number: 72,
      pbi_id: "PBI-094",
      goal:
        "Documentation authors can continue using go-to-definition after rust-analyzer crashes",
      status: "done",
      subtasks: [
        {
          test:
            "LanguageServerConnection.is_alive() returns true for live process, false after kill",
          implementation:
            "Add is_alive() method using process.try_wait() to check process state",
          type: "behavioral",
          status: "completed",
          commits: [
            {
              hash: "ea2028c",
              message:
                "test(lsp): add is_alive() to LanguageServerConnection for crash detection",
              phase: "green",
            },
          ],
          notes: [],
        },
        {
          test:
            "RustAnalyzerPool.take_connection() respawns when existing connection is dead",
          implementation:
            "Check is_alive() before returning; if dead, spawn fresh connection",
          type: "behavioral",
          status: "completed",
          commits: [
            {
              hash: "13ec61c",
              message:
                "feat(lsp): implement automatic crash recovery for rust-analyzer pool",
              phase: "green",
            },
          ],
          notes: [],
        },
      ],
    },
    {
      number: 71,
      pbi_id: "PBI-090",
      goal: "Developer can see syntax highlighting for standalone Lua files",
      status: "done",
      subtasks: [],
    },
  ],

  // Recent 2 retrospectives | Sprint 1-69: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    {
      sprint: 72,
      improvements: [
        {
          action:
            "Extract rust-analyzer availability check into test helper function",
          timing: "immediate",
          status: "completed",
          outcome:
            "Reduced duplication from 32 lines to 13 lines; commit 201f254",
        },
        {
          action:
            "Fix hang on repeated go-to-definition by using didChange for already-open documents",
          timing: "immediate",
          status: "completed",
          outcome:
            "Tracked document_version, send didChange on reuse; commit ee78d2b",
        },
        {
          action: "Add E2E test for go-to-definition after rust-analyzer crash",
          timing: "product",
          status: "active",
          outcome: null,
        },
      ],
    },
    {
      sprint: 71,
      improvements: [
        {
          action:
            "Document Tower-LSP concurrent request processing pattern in CLAUDE.md",
          timing: "immediate",
          status: "completed",
          outcome:
            "Added section on race conditions, solution patterns, and guidelines for new LSP handlers",
        },
        {
          action: "Add unit tests for concurrent LSP request scenarios",
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
