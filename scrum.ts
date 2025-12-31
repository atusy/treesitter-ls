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

  // Completed: PBI-001 to PBI-123 | History: git log -- scrum.yaml, scrum.ts
  // Deferred: PBI-091 (idle cleanup), PBI-107 (WorkspaceType removal)
  product_backlog: [
    {
      id: "PBI-124",
      story: { role: "Rustacean editing Markdown", capability: "global bridge defaults at languages._", benefit: "one default for all hosts" },
      acceptance_criteria: [
        { criterion: "languages._ key supported", verification: "languages._.bridge._ applies globally" },
        { criterion: "Four-level cascade", verification: "host.inj > host._ > _.inj > _._" },
        { criterion: "Documentation", verification: "Cascade table in README" },
      ],
      status: "draft", // Depends on PBI-123
    },
    {
      id: "PBI-125",
      story: { role: "Rustacean editing Markdown", capability: "bridge 'mode' for injection handling", benefit: "optimize for isolated vs context-aware" },
      acceptance_criteria: [
        { criterion: "mode field with defined semantics", verification: "Config + docs" },
        { criterion: "E2E tests verify mode behavior", verification: "make test_nvim" },
      ],
      status: "draft", // Needs refinement: separate vs merged semantics unclear
    },
  ],

  sprint: null, // Sprint 110 completed - PBI-123 done

  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },

  // History: git log -- scrum.yaml, scrum.ts | Completed PBIs: 001-123 (PBI-123 completed Sprint 110)
  // Sprint 110: PBI-123 (bridge per injection with '_' wildcard defaults) - DONE
  completed: [
    { number: 110, pbi_id: "PBI-123", goal: "Bridge per injection with '_' wildcard defaults", status: "done", subtasks: [] },
    { number: 109, pbi_id: "PBI-122", goal: "Add top-level languageServers field with bridge.servers deprecation", status: "done", subtasks: [] },
    { number: 108, pbi_id: "PBI-121", goal: "Extract selection_range/signature_help to modules (PBI-121 DONE)", status: "done", subtasks: [] },
    { number: 107, pbi_id: "PBI-121", goal: "Extract code_action to code_action.rs", status: "done", subtasks: [] },
    { number: 106, pbi_id: "PBI-121", goal: "Extract formatting to formatting.rs", status: "done", subtasks: [] },
    { number: 105, pbi_id: "PBI-121", goal: "Extract rename to rename.rs", status: "done", subtasks: [] },
    { number: 104, pbi_id: "PBI-121", goal: "Extract references to references.rs", status: "done", subtasks: [] },
    { number: 103, pbi_id: "PBI-121", goal: "Extract goto_definition to definition.rs", status: "done", subtasks: [] },
    { number: 102, pbi_id: "PBI-121", goal: "Extract hover to hover.rs", status: "done", subtasks: [] },
    { number: 101, pbi_id: "PBI-121", goal: "Extract completion to completion.rs", status: "done", subtasks: [] },
    { number: 100, pbi_id: "PBI-121", goal: "Extract semantic_tokens to dedicated module", status: "done", subtasks: [] },
    // Sprints 1-99: See git log -- scrum.yaml, scrum.ts
  ],
  retrospectives: [
    {
      sprint: 110,
      improvements: [
        { action: "Serde untagged enum enabled backwards-compatible schema change; TDD drove union type design", timing: "immediate", status: "completed", outcome: "BridgeConfig Array|Map enum with #[serde(untagged)] deserializes both formats seamlessly" },
      ],
    },
    {
      sprint: 109,
      improvements: [
        { action: "First behavioral PBI after 9 structural sprints; strict TDD (6 test cycles) delivered clean API", timing: "immediate", status: "completed", outcome: "PBI-122 done: languageServers field + deprecation path; effective_language_servers() merges both sources" },
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
