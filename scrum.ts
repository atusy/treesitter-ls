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

  // Completed PBIs: PBI-001 through PBI-108 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  // PBI-107 (remove WorkspaceType): Deferred - rust-analyzer linkedProjects approach too slow for E2E tests
  product_backlog: [
    {
      id: "PBI-108",
      story: {
        role: "documentation author with Rust code blocks",
        capability:
          "configure which injection languages are bridged per host document type",
        benefit:
          "I can have R-only bridging in Rmd files while bridging both Python and R in Quarto files, avoiding unnecessary server spawns and tailoring LSP features to my workflow",
      },
      acceptance_criteria: [
        {
          criterion:
            "languages.<filetype>.bridge accepts an array of language names to bridge only those languages",
          verification:
            "cargo test test_bridge_filter_allows_specified_languages",
        },
        {
          criterion:
            "languages.<filetype>.bridge: [] disables all bridging for that host filetype",
          verification: "cargo test test_bridge_filter_empty_disables_bridging",
        },
        {
          criterion:
            "languages.<filetype>.bridge omitted or null bridges all configured languages (default behavior)",
          verification:
            "cargo test test_bridge_filter_null_bridges_all_languages",
        },
        {
          criterion:
            "Bridge filtering is applied at request time before routing to language servers",
          verification:
            "cargo test test_bridge_router_respects_host_filter",
        },
      ],
      status: "done",
    },
  ],

  sprint: {
    number: 85,
    pbi_id: "PBI-108",
    goal:
      "Add per-host language bridge filter configuration to control which injection languages are bridged",
    status: "done",
    subtasks: [
      {
        test: "LanguageConfig parses bridge field as Option<Vec<String>> - test with array ['python', 'r'], empty array [], and null/omitted",
        implementation:
          "Add 'bridge: Option<Vec<String>>' field to LanguageConfig struct in settings.rs with serde deserialization",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "278c076",
            message:
              "feat(config): add per-host language bridge filter for injection redirection",
            phase: "green",
          },
        ],
        notes: [
          "AC1: languages.<filetype>.bridge accepts array of language names",
          "AC2: empty array disables bridging",
          "AC3: null/omitted bridges all",
          "Already implemented - tests exist in settings.rs",
        ],
      },
      {
        test: "LanguageSettings domain type includes bridge field - verify conversion from LanguageConfig preserves bridge filter",
        implementation:
          "Add 'bridge: Option<Vec<String>>' to LanguageSettings struct and update constructor/conversion",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "278c076",
            message:
              "feat(config): add per-host language bridge filter for injection redirection",
            phase: "green",
          },
        ],
        notes: [
          "Domain layer needs the bridge filter to pass to LSP layer",
          "Already implemented - field exists and with_bridge constructor works",
        ],
      },
      {
        test: "WorkspaceSettings.languages map propagates bridge field from TreeSitterSettings parsing",
        implementation:
          "Update TreeSitterSettings to WorkspaceSettings conversion to include bridge field in LanguageSettings",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "278c076",
            message:
              "feat(config): add per-host language bridge filter for injection redirection",
            phase: "green",
          },
        ],
        notes: [
          "Conversion in config.rs From implementations",
          "Already implemented - bridge field propagates through all conversions",
        ],
      },
      {
        test: "is_language_bridgeable(host_lang, injection_lang) returns true when bridge is None (default bridges all)",
        implementation:
          "Add is_language_bridgeable helper function that checks bridge filter - None means bridge all",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "278c076",
            message:
              "feat(config): add per-host language bridge filter for injection redirection",
            phase: "green",
          },
        ],
        notes: [
          "AC3 verification: null/omitted bridges all configured languages",
          "Test: test_bridge_filter_null_bridges_all_languages",
        ],
      },
      {
        test: "is_language_bridgeable returns false when bridge is empty array []",
        implementation:
          "Extend is_language_bridgeable to return false for empty bridge array",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "278c076",
            message:
              "feat(config): add per-host language bridge filter for injection redirection",
            phase: "green",
          },
        ],
        notes: [
          "AC2 verification: empty array disables bridging",
          "Test: test_bridge_filter_empty_disables_bridging",
        ],
      },
      {
        test: "is_language_bridgeable returns true only when injection language is in bridge array",
        implementation:
          "Complete is_language_bridgeable to check if injection_lang is contained in bridge array",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "278c076",
            message:
              "feat(config): add per-host language bridge filter for injection redirection",
            phase: "green",
          },
        ],
        notes: [
          "AC1 verification: bridge only specified languages",
          "Test: test_bridge_filter_allows_specified_languages",
        ],
      },
      {
        test: "get_bridge_config_for_language respects host document's bridge filter before returning config",
        implementation:
          "Modify get_bridge_config_for_language in lsp_impl.rs to take host_language parameter and check is_language_bridgeable",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "278c076",
            message:
              "feat(config): add per-host language bridge filter for injection redirection",
            phase: "green",
          },
        ],
        notes: [
          "AC4 verification: Bridge filtering applied at request time before routing",
          "Returns None if injection language not allowed for host",
          "Test: test_bridge_router_respects_host_filter",
        ],
      },
      {
        test: "eager_spawn_for_injections respects bridge filter - only spawns servers for allowed injection languages",
        implementation:
          "Update eager_spawn_for_injections to filter injection languages through is_language_bridgeable before spawning",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "278c076",
            message:
              "feat(config): add per-host language bridge filter for injection redirection",
            phase: "green",
          },
        ],
        notes: [
          "Prevents unnecessary server spawns for disallowed bridges",
          "Uses host document language to lookup bridge filter",
          "Filter checked via get_bridge_config_for_language",
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
      number: 84,
      pbi_id: "PBI-107",
      goal:
        "Remove WorkspaceType - treesitter-ls creates only virtual.<ext> file per ADR-0006 Minimal File Creation",
      status: "cancelled",
      subtasks: [],
    },
    {
      number: 83,
      pbi_id: "PBI-106",
      goal:
        "Simplify BridgeServerConfig by merging 'command' and 'args' into single 'cmd' array",
      status: "done",
      subtasks: [],
    },
  ],

  // Recent 2 retrospectives | Sprint 1-77: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    {
      sprint: 84,
      improvements: [
        {
          action:
            "Validate external tool initialization time before removing working scaffolding - rust-analyzer linkedProjects takes much longer than Cargo.toml approach",
          timing: "immediate",
          status: "completed",
          outcome:
            "E2E tests revealed linkedProjects initialization too slow; workspaceType kept and marked deprecated in ADR-0006 for future removal",
        },
        {
          action:
            "When simplifying config, ensure the alternative approach actually works in practice - theoretical ADR alignment should not override practical functionality",
          timing: "immediate",
          status: "completed",
          outcome:
            "Sprint cancelled after discovering linkedProjects approach causes test timeouts; pragmatic decision to defer removal",
        },
      ],
    },
    {
      sprint: 83,
      improvements: [
        {
          action:
            "Update ADRs alongside code changes - ADR-0006 was outdated with old command+args schema",
          timing: "immediate",
          status: "completed",
          outcome:
            "ADR-0006 updated to reflect new cmd array format; documentation stays in sync with implementation",
        },
        {
          action:
            "Simpler config schema reduces user confusion - cmd array matches vim.lsp.config pattern",
          timing: "immediate",
          status: "completed",
          outcome:
            "Users now write cmd = { 'rust-analyzer' } instead of command = 'rust-analyzer' with optional args",
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
