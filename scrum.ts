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
      "Expand LSP bridge to support most language server features indirectly through bridging (ADR-0006, 0007, 0008)",
    success_metrics: [
      {
        metric: "Bridge coverage",
        target:
          "Support completion, signatureHelp, references, rename, codeAction, formatting",
      },
      {
        metric: "Modular architecture",
        target: "Bridge module split into per-feature files for maintainability",
      },
      {
        metric: "E2E test coverage",
        target: "Each bridged feature has E2E test verifying end-to-end flow",
      },
    ],
  },

  // Completed PBIs: PBI-001 through PBI-121 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  // PBI-107 (remove WorkspaceType): Deferred - rust-analyzer linkedProjects too slow
  product_backlog: [
    {
      id: "PBI-120",
      story: {
        role: "Rustacean editing Markdown",
        capability: "configure per-language bridge filters using a map structure with enabled flag",
        benefit: "I can explicitly enable/disable bridging for specific injection languages with room for future per-language options",
      },
      acceptance_criteria: [
        { criterion: "LanguageConfig.bridge accepts map structure: { 'python': { 'enabled': true } }", verification: "cargo test should_parse_language_config_with_bridge_map passes" },
        { criterion: "BridgeLanguageConfig struct exists with 'enabled' field", verification: "grep 'pub struct BridgeLanguageConfig' src/config/settings.rs returns matches" },
        { criterion: "is_language_bridgeable method checks enabled field in the map", verification: "cargo test test_bridge_filter_map_enabled passes" },
        { criterion: "README.md updated to show new bridge map configuration schema", verification: "grep 'enabled' README.md returns matches in bridge examples" },
        { criterion: "E2E tests pass with bridge map configuration", verification: "make test_nvim passes" },
      ],
      status: "ready",
    },
  ],

  sprint: null, // Sprint 98 (PBI-121) completed - lsp_impl modular refactoring

  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },

  // Historical sprints (recent 2) | Sprint 1-96: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 98, pbi_id: "PBI-121", goal: "Refactor lsp_impl.rs into modular file structure", status: "done", subtasks: [] },
    { number: 97, pbi_id: "PBI-120", goal: "Bridge filter map with enabled flag", status: "cancelled", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-96: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    {
      sprint: 98,
      improvements: [
        { action: "Modular refactoring with *_impl delegation decomposed 3800+ line file into 10 focused text_document modules", timing: "immediate", status: "completed", outcome: "pub(crate) *_impl methods called from LanguageServer trait impl" },
        { action: "File organization by LSP category (text_document/) creates natural boundaries for future workspace/ and window/", timing: "product", status: "active", outcome: null },
      ],
    },
    {
      sprint: 96,
      improvements: [
        { action: "Schema simplification - languageServers at root level", timing: "immediate", status: "completed", outcome: "BridgeSettings wrapper removed; all E2E tests passing" },
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
