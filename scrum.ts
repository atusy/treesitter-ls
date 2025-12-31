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
      id: "PBI-123",
      story: { role: "Rustacean editing Markdown", capability: "configure bridge per injection with '_' wildcard defaults", benefit: "set defaults once, override when needed" },
      acceptance_criteria: [
        { criterion: "bridge changes from array to map with '_' key", verification: "bridge._ = {enabled: true} works" },
        { criterion: "BridgeLanguageConfig with 'enabled' field", verification: "bridge.rust = {enabled: false} works" },
        { criterion: "Cascade: host.injection > host._", verification: "Unit tests verify inheritance" },
        { criterion: "Backwards compat: array format works", verification: "effective_bridge() handles both" },
        { criterion: "Documentation updated", verification: "README.md migration examples" },
      ],
      status: "done",
    },
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

  sprint: {
    number: 110,
    pbi_id: "PBI-123",
    goal: "Bridge per injection with '_' wildcard defaults",
    status: "done",
    subtasks: [
      {
        test: "should_deserialize_bridge_language_config: BridgeLanguageConfig with 'enabled' field deserializes",
        implementation: "Create BridgeLanguageConfig struct with enabled: bool field",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "5c1a1ea", message: "feat(config): add BridgeLanguageConfig struct", phase: "green" }],
        notes: ["TDD Step 1: Create BridgeLanguageConfig type with 'enabled' field"],
      },
      {
        test: "should_deserialize_bridge_map_with_underscore: bridge accepts map with '_' key",
        implementation: "Create BridgeConfig type as HashMap<String, BridgeLanguageConfig> accepting '_' key",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "f59f67a", message: "feat(config): add BridgeConfig enum with Array|Map variants", phase: "green" }],
        notes: ["TDD Step 2: Create new bridge map type that accepts '_' key"],
      },
      {
        test: "effective_bridge_resolves_specific_over_default: rust config takes precedence over '_' default",
        implementation: "Implement cascade resolution: specific language > '_' default",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "c92573a", message: "feat(config): implement cascade resolution for bridge config", phase: "green" }],
        notes: ["TDD Step 3: Implement cascade resolution - specific > default ('_')"],
      },
      {
        test: "effective_bridge_handles_array_format: array format ['rust', 'python'] still works",
        implementation: "BridgeConfig enum supports both Array and Map; is_language_bridgeable() handles both",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "c92573a", message: "feat(config): implement cascade resolution for bridge config", phase: "green" }],
        notes: ["TDD Step 4: Backwards compat already implemented via BridgeConfig enum"],
      },
      {
        test: "uses_deprecated_bridge_array_detects_old_format: Returns true when bridge is array",
        implementation: "Add deprecation detection and log::warn for array format",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "5ad8a28", message: "feat(config): add deprecation detection for bridge array format", phase: "green" }],
        notes: ["TDD Step 5: Add deprecation warning for array format"],
      },
      {
        test: "N/A - Documentation update",
        implementation: "Update README.md with bridge map format and migration examples",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "dc02fb7", message: "docs(config): document bridge map format with migration guide", phase: "green" }],
        notes: ["TDD Step 6: Update documentation with Before/After examples"],
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
        { action: "Strict TDD (6 cycles) delivered BridgeConfig enum with backwards compat + deprecation path", timing: "immediate", status: "completed", outcome: "PBI-123 done: bridge map with '_' wildcard, cascade resolution, deprecation warning for array format" },
      ],
    },
    {
      sprint: 109,
      improvements: [
        { action: "First behavioral PBI after 9 structural sprints; strict TDD (6 test cycles) delivered clean API", timing: "immediate", status: "completed", outcome: "PBI-122 done: languageServers field + deprecation path; effective_language_servers() merges both sources" },
      ],
    },
    {
      sprint: 108,
      improvements: [
        { action: "PBI-121 COMPLETE: 10 LSP method modules extracted over 9 sprints (100-108)", timing: "immediate", status: "completed", outcome: "lsp_impl.rs reduced from ~1800 to ~1500 lines; text_document/ now has 10 focused modules" },
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
