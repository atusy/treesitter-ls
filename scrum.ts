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

  // Completed PBIs: PBI-001 through PBI-120 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  // PBI-107 (remove WorkspaceType): Deferred - rust-analyzer linkedProjects too slow
  product_backlog: [
    {
      id: "PBI-121",
      story: { role: "Rustacean editing Markdown", capability: "maintain lsp_impl.rs through modular file structure", benefit: "easier to navigate and modify without merge conflicts" },
      acceptance_criteria: [
        { criterion: "Phase 1: semantic_tokens → semantic_tokens.rs", verification: "Tests pass; DONE in Sprint 100" },
        { criterion: "Phase 2: completion → completion.rs", verification: "Tests pass; DONE in Sprint 101" },
        { criterion: "Phase 3: hover → hover.rs", verification: "Tests pass; DONE in Sprint 102" },
        { criterion: "Phase 4: goto_definition → definition.rs", verification: "Tests pass; DONE in Sprint 103" },
        { criterion: "Phase 5: references → references.rs", verification: "Tests pass; DONE in Sprint 104" },
        { criterion: "Phase 6: rename → rename.rs", verification: "Tests pass; DONE in Sprint 105" },
        { criterion: "Phase 7: formatting → formatting.rs", verification: "Tests pass; DONE in Sprint 106" },
        { criterion: "Phase 8: code_action → code_action.rs", verification: "Tests pass; DONE in Sprint 107" },
        { criterion: "Phase 9: selection_range, signature_help → respective modules", verification: "Tests pass; mod.rs re-exports complete" },
      ],
      status: "ready",
    },
    {
      id: "PBI-122",
      story: { role: "Rustacean editing Markdown", capability: "configure bridge servers at top-level 'languageServers'", benefit: "flatter config, clearer field name" },
      acceptance_criteria: [
        { criterion: "languageServers field added to schema", verification: "Config works; unit tests verify" },
        { criterion: "bridge.servers deprecated but functional", verification: "effective_language_servers() merges both" },
        { criterion: "Deprecation warning logged", verification: "log_deprecation_warnings() warns" },
        { criterion: "Documentation updated", verification: "README.md Before/After examples" },
      ],
      status: "ready",
    },
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
      status: "ready",
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
    number: 108,
    pbi_id: "PBI-121",
    goal: "Extract selection_range and signature_help to respective modules",
    status: "planning",
    subtasks: [],
  },

  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },

  // History: git log -- scrum.yaml, scrum.ts | Completed PBIs: 001-120
  completed: [
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
  // Sprint 108: Final extraction in PBI-121 series (selection_range.rs, signature_help.rs)
  retrospectives: [
    {
      sprint: 107,
      improvements: [
        { action: "8th consecutive successful extraction (code_action.rs); only 1 phase remains (selection_range/signature_help)", timing: "immediate", status: "completed", outcome: "PBI-121 on track for completion in Sprint 108" },
      ],
    },
    {
      sprint: 106,
      improvements: [
        { action: "7th consecutive successful extraction; only 2 phases remain (code_action, selection_range/signature_help)", timing: "immediate", status: "completed", outcome: "PBI-121 on track for completion in Sprint 108" },
        { action: "E2E naming issue (treesitter_ls vs treesitter-ls) abandoned after 4 sprints - non-blocking, no impact on delivery", timing: "immediate", status: "abandoned", outcome: "Cleaned up stale improvement item" },
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
