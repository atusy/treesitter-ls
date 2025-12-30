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
      status: "ready",
    },
    {
      id: "PBI-100",
      story: {
        role: "documentation author with Rust code blocks",
        capability:
          "configure workspace setup per bridge server type (e.g., Cargo.toml for rust-analyzer, venv for pyright)",
        benefit:
          "each language server gets the project structure it needs without hard-coded assumptions",
      },
      acceptance_criteria: [
        {
          criterion:
            "BridgeServerConfig accepts optional workspace_type field",
          verification:
            "Test that workspace_type can be 'cargo', 'generic', or custom type",
        },
        {
          criterion:
            "Spawn creates appropriate workspace structure based on workspace_type",
          verification:
            "Test that cargo type creates Cargo.toml, generic type creates empty workspace",
        },
        {
          criterion:
            "Default workspace_type is 'generic' for non-rust servers",
          verification:
            "Test that pyright config without workspace_type uses generic workspace",
        },
      ],
      status: "draft",
    },
  ],

  sprint: {
    number: 76,
    pbi_id: "PBI-099",
    goal:
      "Documentation authors have stale temp files cleaned up automatically on startup, preventing temp directory pollution from crashed sessions",
    status: "in_progress",
    subtasks: [
      {
        test: "Test cleanup_stale_temp_dirs function exists and can be called",
        implementation:
          "Create cleanup_stale_temp_dirs function signature in src/lsp/redirection.rs that takes temp_dir path and max_age duration",
        type: "behavioral",
        status: "green",
        commits: [],
        notes: [
          "Function signature: cleanup_stale_temp_dirs(temp_dir: &Path, max_age: Duration) -> io::Result<CleanupStats>",
          "CleanupStats tracks dirs_removed and dirs_kept for logging",
        ],
      },
      {
        test: "Test cleanup identifies directories matching treesitter-ls-* prefix",
        implementation:
          "Implement directory scanning with prefix matching in cleanup_stale_temp_dirs",
        type: "behavioral",
        status: "green",
        commits: [],
        notes: [
          "Only scan for directories with treesitter-ls- prefix",
          "Use std::fs::read_dir to list temp directory contents",
        ],
      },
      {
        test: "Test cleanup removes directories older than max_age threshold",
        implementation:
          "Add age check using directory metadata modified time, remove old directories",
        type: "behavioral",
        status: "green",
        commits: [],
        notes: [
          "Use metadata().modified() to get directory age",
          "Default max_age is 24 hours (Duration::from_secs(24 * 60 * 60))",
          "Use std::fs::remove_dir_all for removal",
        ],
      },
      {
        test: "Test cleanup keeps directories newer than max_age threshold",
        implementation:
          "Verify age comparison logic preserves recent directories",
        type: "behavioral",
        status: "green",
        commits: [],
        notes: [
          "Create test with fresh directory that should be kept",
          "Verify it remains after cleanup runs",
        ],
      },
      {
        test: "Test cleanup continues gracefully when permission denied on some directories",
        implementation:
          "Wrap removal in error handling that logs and continues",
        type: "behavioral",
        status: "green",
        commits: [],
        notes: [
          "Use log::warn! for removal failures",
          "Return Ok even if some removals fail",
          "Track failed removals in CleanupStats",
        ],
      },
      {
        test: "Test startup calls cleanup function during initialization",
        implementation:
          "Call cleanup_stale_temp_dirs from TreeSitterLs::new or initialize method",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Call cleanup in background to avoid blocking startup",
          "Use std::thread::spawn or tokio::spawn for async cleanup",
          "Log cleanup results at info level",
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

  // Historical sprints (recent 2) | Sprint 1-72: git log -- scrum.yaml, scrum.ts
  completed: [
    {
      number: 75,
      pbi_id: "PBI-097",
      goal:
        "Documentation authors can configure bridge servers via initializationOptions for multi-language LSP support",
      status: "done",
      subtasks: [],
    },
    {
      number: 74,
      pbi_id: "PBI-096",
      goal:
        "Documentation authors see progress indicators during rust-analyzer operations",
      status: "done",
      subtasks: [],
    },
  ],

  // Recent 2 retrospectives | Sprint 1-72: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    {
      sprint: 75,
      improvements: [],
    },
    {
      sprint: 74,
      improvements: [
        {
          action:
            "Document spawn_blocking + synchronous methods pattern for external language server communication in CLAUDE.md",
          timing: "sprint",
          status: "active",
          outcome: null,
        },
        {
          action:
            "Consider adding timeout tests at tokio::time::timeout level for spawn_blocking calls",
          timing: "sprint",
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
