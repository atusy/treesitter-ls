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
        { criterion: "Phase 9: selection_range, signature_help → respective modules", verification: "Tests pass; DONE in Sprint 108" },
      ],
      status: "done",
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
    status: "done",
    subtasks: [
      {
        test: "Verify baseline: make test, make check pass before changes",
        implementation: "Run baseline verification to ensure clean starting point",
        type: "structural",
        status: "completed",
        commits: [],
        notes: ["Final phase of PBI-121 refactoring - last 2 methods to extract"],
      },
      {
        test: "Verify selection_range module compiles with mod declaration in text_document.rs",
        implementation: "Create selection_range.rs with module structure and add 'pub mod selection_range;' to text_document.rs",
        type: "structural",
        status: "completed",
        commits: [],
        notes: ["Follow same pattern as code_action.rs, formatting.rs, etc."],
      },
      {
        test: "Verify selection_range tests pass after method move",
        implementation: "Move selection_range method from lsp_impl.rs to selection_range.rs, update imports",
        type: "structural",
        status: "completed",
        commits: [],
        notes: ["Method at lines 1281-1356 in lsp_impl.rs"],
      },
      {
        test: "Verify signature_help module compiles with mod declaration in text_document.rs",
        implementation: "Create signature_help.rs with module structure and add 'pub mod signature_help;' to text_document.rs",
        type: "structural",
        status: "completed",
        commits: [],
        notes: ["Follow same pattern as selection_range.rs"],
      },
      {
        test: "Verify signature_help tests pass after method move",
        implementation: "Move signature_help method from lsp_impl.rs to signature_help.rs, update imports",
        type: "structural",
        status: "completed",
        commits: [],
        notes: ["Method at lines 1377-1541 in lsp_impl.rs"],
      },
      {
        test: "All DoD checks pass: make test, make check, make test_nvim",
        implementation: "Run full verification suite to confirm PBI-121 completion",
        type: "structural",
        status: "completed",
        commits: [],
        notes: ["PBI-121 Phase 9 complete - text_document.rs now exports all 10 LSP modules"],
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

  // History: git log -- scrum.yaml, scrum.ts | Completed PBIs: 001-121
  completed: [
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
  // Sprint 109: Start PBI-122 (languageServers top-level config)
  retrospectives: [
    {
      sprint: 108,
      improvements: [
        { action: "PBI-121 COMPLETE: 10 LSP method modules extracted over 9 sprints (100-108)", timing: "immediate", status: "completed", outcome: "lsp_impl.rs reduced from ~1800 to ~1500 lines; text_document/ now has 10 focused modules" },
      ],
    },
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
