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
      status: "done",
    },
    {
      id: "PBI-101",
      story: {
        role: "documentation author with Rust code blocks",
        capability:
          "have spawn() use the workspace_type configuration to create appropriate workspace structure",
        benefit:
          "the workspace type feature works end-to-end, not just as infrastructure",
      },
      acceptance_criteria: [
        {
          criterion:
            "LanguageServerConnection::spawn() uses setup_workspace() based on config.workspace_type",
          verification:
            "Integration test: spawn with generic workspace_type creates virtual file structure",
        },
        {
          criterion:
            "LanguageServerConnection stores ConnectionInfo for virtual file operations",
          verification:
            "Unit test: connection.virtual_file_uri() returns correct path after spawn",
        },
        {
          criterion:
            "did_open() uses ConnectionInfo.write_virtual_file() instead of hardcoded path",
          verification:
            "Integration test: did_open writes to generic workspace virtual file correctly",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-102",
      story: {
        role: "documentation author with Rust code blocks",
        capability:
          "have bridge server connections pre-warmed when I open a document",
        benefit:
          "my first go-to-definition request is fast instead of waiting for server startup",
      },
      acceptance_criteria: [
        {
          criterion:
            "When did_open is called for an injection region, the bridge server connection is spawned in a background thread",
          verification:
            "Unit test: did_open triggers background spawn for configured bridge server language",
        },
        {
          criterion:
            "The eager spawn does not block did_open completion",
          verification:
            "Unit test: did_open returns immediately while spawn continues in background",
        },
        {
          criterion:
            "The eagerly spawned connection is stored in LanguageServerPool for reuse",
          verification:
            "Integration test: after did_open completes, pool contains connection for the language",
        },
        {
          criterion:
            "goto_definition uses the pre-warmed connection instead of spawning a new one",
          verification:
            "Integration test: first goto_definition after did_open reuses existing connection from pool",
        },
      ],
      status: "done",
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

  // Historical sprints (recent 2) | Sprint 1-72: git log -- scrum.yaml, scrum.ts
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

  // Recent 2 retrospectives | Sprint 1-72: git log -- scrum.yaml, scrum.ts
  retrospectives: [
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
    {
      sprint: 77,
      improvements: [
        {
          action:
            "Integrate ConnectionInfo and setup_workspace() into LanguageServerConnection::spawn() to complete the workspace type feature end-to-end",
          timing: "product",
          status: "completed",
          outcome:
            "Completed in Sprint 78 (PBI-101): spawn() now uses setup_workspace_with_option() based on config.workspace_type, stores ConnectionInfo for virtual file operations, and did_open/goto_definition/hover all use virtual_file_uri().",
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
