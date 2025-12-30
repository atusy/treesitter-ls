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

  // Completed PBIs: PBI-001 through PBI-096 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  product_backlog: [
    {
      id: "PBI-097",
      story: {
        role: "documentation author with Rust code blocks",
        capability:
          "configure bridge servers via initializationOptions so I can use different language servers beyond rust-analyzer",
        benefit:
          "I can get LSP features in Python, Go, TypeScript code blocks using my preferred language servers",
      },
      acceptance_criteria: [
        {
          criterion:
            "User can specify bridge server configuration in LSP initializationOptions",
          verification:
            "Test that bridge.servers config is parsed from initializationOptions",
        },
        {
          criterion:
            "Server configuration includes command, args, languages, and initializationOptions",
          verification:
            "Test that all config fields are passed through to spawned server",
        },
        {
          criterion:
            "Connections spawn the configured command instead of hard-coded rust-analyzer",
          verification:
            "Test that spawn uses command from config, not hard-coded binary name",
        },
        {
          criterion:
            "Servers receive user-provided initializationOptions during initialize",
          verification:
            "Test that linkedProjects and other server options are passed through",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-098",
      story: {
        role: "documentation author with Rust code blocks",
        capability:
          "have the bridge route requests to the correct server based on injection language",
        benefit:
          "I get proper LSP features for each language in my mixed-language documents",
      },
      acceptance_criteria: [
        {
          criterion:
            "Bridge routes requests to server configured for the injection language",
          verification:
            "Test rust injection goes to rust-analyzer, python to pyright",
        },
        {
          criterion:
            "Server pool is keyed by server name, not just 'rust-analyzer'",
          verification:
            "Test that multiple servers can be pooled simultaneously",
        },
        {
          criterion:
            "Graceful fallback when no server is configured for a language",
          verification:
            "Test that missing server config returns None without error",
        },
      ],
      status: "refining",
    },
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
      status: "refining",
    },
  ],

  sprint: {
    number: 75,
    pbi_id: "PBI-097",
    goal:
      "Documentation authors can configure bridge servers via initializationOptions for multi-language LSP support",
    status: "in_progress",
    subtasks: [
      {
        test: "Test that BridgeServerConfig struct can deserialize command, args, languages, and initializationOptions fields",
        implementation:
          "Create BridgeServerConfig struct with serde derives in src/config/settings.rs",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "e7b75b8", message: "feat(config): add BridgeServerConfig struct for bridge server configuration", phase: "green" }],
        notes: [
          "Define: command: String, args: Option<Vec<String>>, languages: Vec<String>, initialization_options: Option<serde_json::Value>",
        ],
      },
      {
        test: "Test that BridgeSettings struct deserializes from bridge.servers map",
        implementation:
          "Create BridgeSettings with servers: HashMap<String, BridgeServerConfig> in settings.rs",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "46b4e6f", message: "feat(config): add BridgeSettings struct for server map configuration", phase: "green" }],
        notes: [
          "JSON schema: { bridge: { servers: { 'rust-analyzer': { command: '...', ... } } } }",
        ],
      },
      {
        test: "Test that TreeSitterSettings includes optional bridge field that deserializes correctly",
        implementation:
          "Add bridge: Option<BridgeSettings> field to TreeSitterSettings",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "0a43fc7", message: "feat(config): add optional bridge field to TreeSitterSettings", phase: "green" }],
        notes: [
          "Ensure backward compatibility - missing bridge field should parse to None",
        ],
      },
      {
        test: "Test that LanguageServerConnection::spawn accepts BridgeServerConfig and uses command from config",
        implementation:
          "Refactor spawn_rust_analyzer to generic spawn(config: &BridgeServerConfig) method",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "e0a0cc5", message: "feat(lsp): add generic spawn method accepting BridgeServerConfig", phase: "green" }],
        notes: [
          "Keep spawn_rust_analyzer as convenience wrapper or deprecate entirely",
        ],
      },
      {
        test: "Test that spawn passes args from config to Command::new",
        implementation:
          "Add .args() call using config.args.unwrap_or_default()",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "e0a0cc5", message: "feat(lsp): add generic spawn method accepting BridgeServerConfig", phase: "green" }],
        notes: [
          "Some language servers need specific args like --stdio or --lsp",
        ],
      },
      {
        test: "Test that spawn passes initializationOptions from config in initialize request",
        implementation:
          "Include config.initialization_options in init_params JSON",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "e0a0cc5", message: "feat(lsp): add generic spawn method accepting BridgeServerConfig", phase: "green" }],
        notes: [
          "rust-analyzer uses linkedProjects, pyright uses venvPath, etc.",
        ],
      },
      {
        test: "Test that RustAnalyzerPool is replaced with generic LanguageServerPool keyed by server name",
        implementation:
          "Rename RustAnalyzerPool to LanguageServerPool, update pool key usage",
        type: "structural",
        status: "completed",
        commits: [{ hash: "1949c5e", message: "refactor(lsp): rename RustAnalyzerPool to LanguageServerPool", phase: "refactoring" }],
        notes: [
          "Structural change - pool behavior remains same, just generalized naming",
        ],
      },
      {
        test: "Test that lsp_impl uses bridge config from settings when spawning language servers",
        implementation:
          "Update lsp_impl to look up server config by name from settings.bridge.servers",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Fall back to hard-coded rust-analyzer config if bridge.servers is empty/missing",
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
      number: 74,
      pbi_id: "PBI-096",
      goal:
        "Documentation authors see progress indicators during rust-analyzer operations",
      status: "done",
      subtasks: [],
    },
    {
      number: 73,
      pbi_id: "PBI-095",
      goal:
        "Documentation authors get responsive go-to-definition even when rust-analyzer is slow",
      status: "done",
      subtasks: [],
    },
  ],

  // Recent 2 retrospectives | Sprint 1-72: git log -- scrum.yaml, scrum.ts
  retrospectives: [
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
    {
      sprint: 73,
      improvements: [
        {
          action:
            "Consider serializing rust-analyzer tests to avoid parallel spawn race conditions",
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
