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
        { criterion: "Phase 5: references → references.rs", verification: "Tests pass" },
        { criterion: "Phase 6: rename → rename.rs", verification: "Tests pass" },
        { criterion: "Phase 7: formatting → formatting.rs", verification: "Tests pass" },
        { criterion: "Phase 8: code_action → code_action.rs", verification: "Tests pass" },
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
    number: 103,
    pbi_id: "PBI-121",
    goal: "Extract goto_definition to definition.rs",
    status: "done",
    subtasks: [
      {
        test: "Verify baseline: make test && make check && make test_nvim pass",
        implementation: "Run all tests to ensure clean starting state",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Baseline verification before any changes"],
      },
      {
        test: "Verify definition.rs module compiles and is declared in mod.rs",
        implementation: "Create src/lsp/lsp_impl/text_document/definition.rs with module declaration in text_document.rs",
        type: "structural",
        status: "completed",
        commits: [],
        notes: ["Add 'pub mod definition;' to text_document.rs"],
      },
      {
        test: "Verify goto_definition method works from new module",
        implementation: "Move goto_definition impl block from lsp_impl.rs to definition.rs; update lsp_impl.rs to delegate: TreeSitterLs::goto_definition(self, params).await",
        type: "structural",
        status: "completed",
        commits: [],
        notes: ["Use pub(crate) visibility; add required imports from hover.rs pattern"],
      },
      {
        test: "Final verification: make test && make check && make test_nvim pass",
        implementation: "Run full test suite to confirm extraction complete",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["All tests must pass before marking sprint complete"],
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

  // History: git log -- scrum.yaml, scrum.ts | Completed PBIs: 001-120
  completed: [
    { number: 103, pbi_id: "PBI-121", goal: "Extract goto_definition to definition.rs", status: "done", subtasks: [] },
    { number: 102, pbi_id: "PBI-121", goal: "Extract hover to hover.rs", status: "done", subtasks: [] },
    { number: 101, pbi_id: "PBI-121", goal: "Extract completion to completion.rs", status: "done", subtasks: [] },
    { number: 100, pbi_id: "PBI-121", goal: "Extract semantic_tokens to dedicated module", status: "done", subtasks: [] },
  ],
  retrospectives: [
    {
      sprint: 103,
      improvements: [
        { action: "Module extraction pattern fully validated (4 consecutive sprints)", timing: "immediate", status: "completed", outcome: "Remaining 5 phases (references, rename, formatting, code_action, selection_range/signature_help) can proceed confidently" },
        { action: "E2E test naming issue (treesitter_ls vs treesitter-ls) unresolved for 4 sprints", timing: "sprint", status: "active", outcome: null },
      ],
    },
    {
      sprint: 102,
      improvements: [
        { action: "Module extraction pattern mature after 3 consecutive sprints", timing: "immediate", status: "completed", outcome: "Pattern is stable: remaining 6 phases can proceed with high confidence" },
        { action: "E2E test naming issue (treesitter_ls vs treesitter-ls) unresolved for 3 sprints", timing: "sprint", status: "active", outcome: null },
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
