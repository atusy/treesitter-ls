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

  // Completed PBIs: PBI-001 through PBI-098 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  // PBI-098: Language-based routing - already implemented as part of PBI-097 (configurable bridge servers)
  product_backlog: [
    {
      id: "PBI-099",
      story: {
        role: "documentation author with Rust code blocks",
        capability:
          "have stale temp files cleaned up on treesitter-ls startup",
        benefit:
          "my temp directory does not fill up with orphaned files from crashed sessions",
      },
      acceptance_criteria: [
        {
          criterion:
            "On startup, treesitter-ls scans for stale temp directories",
          verification:
            "Test that startup calls cleanup function for treesitter-ls temp dirs",
        },
        {
          criterion:
            "Temp directories older than 24 hours are removed",
          verification:
            "Test that old directories are removed, recent ones are kept",
        },
        {
          criterion:
            "Cleanup handles permission errors gracefully",
          verification:
            "Test that cleanup continues even if some files cannot be removed",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-100",
      story: {
        role: "documentation author with Rust code blocks",
        capability:
          "configure workspace setup per bridge server type (e.g., Cargo.toml for rust-analyzer, plain file for pyright)",
        benefit:
          "each language server gets the project structure it needs without hard-coded assumptions",
      },
      acceptance_criteria: [
        {
          criterion:
            "BridgeServerConfig accepts optional workspace_type field with values 'cargo' or 'generic'",
          verification:
            "Unit test: BridgeServerConfig deserializes workspace_type field; None defaults to 'cargo' for backward compatibility",
        },
        {
          criterion:
            "spawn() creates Cargo.toml and src/main.rs when workspace_type is 'cargo' (or None for backward compat)",
          verification:
            "Unit test: spawn with cargo workspace_type creates Cargo.toml and src/main.rs in temp directory",
        },
        {
          criterion:
            "spawn() creates only temp directory with single virtual file when workspace_type is 'generic'",
          verification:
            "Unit test: spawn with generic workspace_type creates temp dir with virtual.<ext> file, no Cargo.toml",
        },
        {
          criterion:
            "main_rs_uri() replaced with virtual_file_uri() that returns appropriate path per workspace_type",
          verification:
            "Unit test: virtual_file_uri returns src/main.rs for cargo, virtual.py for generic python",
        },
        {
          criterion:
            "did_open() writes content to correct virtual file path based on workspace_type",
          verification:
            "Unit test: did_open with generic workspace writes to virtual.<ext> not src/main.rs",
        },
      ],
      status: "ready",
    },
  ],

  sprint: {
    number: 77,
    pbi_id: "PBI-100",
    goal:
      "Documentation authors can configure workspace setup per bridge server type so each language server gets the project structure it needs",
    status: "in_progress",
    subtasks: [
      {
        test: "Unit test: BridgeServerConfig deserializes workspace_type field with values 'cargo' and 'generic'; None defaults to 'cargo'",
        implementation:
          "Add optional workspace_type field to BridgeServerConfig in settings.rs with WorkspaceType enum (Cargo, Generic)",
        type: "behavioral",
        status: "completed",
        commits: [
          { hash: "1f41f3e", message: "test(workspace): add failing tests for WorkspaceType deserialization", phase: "green" as const },
          { hash: "aac9562", message: "feat(workspace): add WorkspaceType enum and workspace_type field", phase: "green" as const },
        ],
        notes: [],
      },
      {
        test: "Unit test: spawn with workspace_type=None or Cargo creates Cargo.toml and src/main.rs in temp directory",
        implementation:
          "Extract cargo workspace setup into helper function; call from spawn() when workspace_type is None or Cargo",
        type: "behavioral",
        status: "red",
        commits: [],
        notes: [],
      },
      {
        test: "Unit test: spawn with workspace_type=Generic creates temp dir with virtual file (no Cargo.toml, no src/)",
        implementation:
          "Add generic workspace setup in spawn() that creates only temp dir with virtual file path stored",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
      },
      {
        test: "Unit test: virtual_file_uri returns src/main.rs for Cargo workspace, virtual.<ext> for Generic workspace",
        implementation:
          "Replace main_rs_uri() with virtual_file_uri() that returns path based on workspace_type and file extension",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
      },
      {
        test: "Unit test: did_open with Generic workspace writes to virtual.<ext> not src/main.rs",
        implementation:
          "Update did_open() to use virtual_file_uri() for file path instead of hardcoded src/main.rs",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
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

  // Historical sprints (recent 2) | Sprint 1-72: git log -- scrum.yaml, scrum.ts
  completed: [
    {
      number: 76,
      pbi_id: "PBI-099",
      goal:
        "Documentation authors have stale temp files cleaned up automatically on startup, preventing temp directory pollution from crashed sessions",
      status: "done",
      subtasks: [],
    },
    {
      number: 75,
      pbi_id: "PBI-097",
      goal:
        "Documentation authors can configure bridge servers via initializationOptions for multi-language LSP support",
      status: "done",
      subtasks: [],
    },
  ],

  // Recent 2 retrospectives | Sprint 1-72: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    {
      sprint: 76,
      improvements: [
        {
          action:
            "Consider extracting cleanup module from redirection.rs if cleanup features grow",
          timing: "product",
          status: "completed",
          outcome:
            "Analyzed: cleanup code has logical connection to redirection (creates temp dirs). Current location acceptable. No action needed unless cleanup features expand significantly.",
        },
      ],
    },
    {
      sprint: 75,
      improvements: [],
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
