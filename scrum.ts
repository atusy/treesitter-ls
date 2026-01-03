// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Rustacean editing Markdown",
  "developer editing Lua files",
  "documentation author with Rust code blocks",
] as const satisfies readonly string[];

const scrum: ScrumDashboard = {
  product_goal: {
    statement:
      "Maintain stable async LSP bridge for core features using single-pool architecture (ADR-0006, 0007, 0008)",
    success_metrics: [
      { metric: "Bridge coverage", target: "Support hover, completion, signatureHelp, definition with fully async implementations" },
      { metric: "Modular architecture", target: "Bridge module organized with text_document/ subdirectory, single TokioAsyncLanguageServerPool" },
      { metric: "E2E test coverage", target: "Each bridged feature has E2E test verifying end-to-end async flow" },
    ],
  },

  // Deferred: PBI-091 (idle cleanup), PBI-107 (WorkspaceType), PBI-171 ($/cancelRequest - tower-lsp internals)
  product_backlog: [
    {
      id: "PBI-172",
      story: {
        role: "developer editing Lua files",
        capability: "relocate smoke tests from integration to unit test location",
        benefit: "improve test pyramid distribution and reduce integration test execution time",
      },
      acceptance_criteria: [
        {
          criterion: "11 smoke tests from test_runtime_coordinator_api.rs moved to src/language/coordinator.rs as inline unit tests",
          verification: "run make test && verify test_runtime_coordinator_api.rs no longer exists",
        },
        {
          criterion: "All moved tests continue to pass with same verification behavior",
          verification: "make test shows all coordinator API tests passing",
        },
        {
          criterion: "Integration test count reduced by ~9%, unit test count increased",
          verification: "cargo test --lib reports increased unit test count",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-173",
      story: {
        role: "Rustacean editing Markdown",
        capability: "parameterize duplicated offset clamping tests using rstest",
        benefit: "reduce test maintenance burden and improve test clarity",
      },
      acceptance_criteria: [
        {
          criterion: "3+ clamping tests in src/analysis/offset_calculator.rs consolidated into 1 parameterized test",
          verification: "grep -c '#\\[test\\]' src/analysis/offset_calculator.rs shows reduced test function count",
        },
        {
          criterion: "Parameterized test uses rstest with named test cases (past_eof, start_past_end, row_past_eof)",
          verification: "cargo test offset_clamping shows 3+ test cases executed",
        },
        {
          criterion: "All original edge cases preserved with identical assertions",
          verification: "make test passes with no regression in coverage",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-174",
      story: {
        role: "documentation author with Rust code blocks",
        capability: "audit and restrict public API visibility in LanguageCoordinator following YAGNI",
        benefit: "prevent unnecessary public API surface expansion and improve encapsulation",
      },
      acceptance_criteria: [
        {
          criterion: "Review all pub methods in src/language/coordinator.rs and change to pub(crate) where not used externally",
          verification: "grep 'pub fn' src/language/coordinator.rs shows reduced public methods",
        },
        {
          criterion: "Add #[cfg(test)] helper methods for test-only accessors if needed",
          verification: "Tests continue to pass with make test",
        },
        {
          criterion: "Document visibility decisions in code comments or ADR",
          verification: "Each method has clear visibility justification",
        },
      ],
      status: "ready",
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

  completed: [
    { number: 144, pbi_id: "PBI-171", goal: "Investigate $/cancelRequest handling via custom_method - blocked by tower-lsp architecture", status: "cancelled", subtasks: [] },
    { number: 143, pbi_id: "PBI-170", goal: "Investigate $/cancelRequest - deferred (tower-lsp limitation, YAGNI)", status: "cancelled", subtasks: [] },
    { number: 142, pbi_id: "PBI-169", goal: "Fix bridge bookkeeping memory leak after crashes/restarts", status: "done", subtasks: [] },
    { number: 141, pbi_id: "PBI-168", goal: "Fix concurrent parse crash recovery to correctly identify failing parsers", status: "done", subtasks: [] },
  ],

  retrospectives: [
    { sprint: 144, improvements: [
      { action: "Investigation: LspServiceBuilder.custom_method cannot intercept $/cancelRequest because tower-lsp registers it first in generated code before custom methods", timing: "product", status: "completed", outcome: "PBI-171 deferred - tower-lsp's Router uses HashMap with first-registration-wins, blocking custom interception" },
      { action: "Current architecture already supports request superseding: new semantic token requests automatically cancel previous ones via SemanticRequestTracker", timing: "product", status: "completed", outcome: "Explicit $/cancelRequest handling deemed unnecessary (YAGNI) - existing superseding mechanism sufficient for user typing scenarios" },
    ] },
    { sprint: 143, improvements: [
      { action: "Review-codex3 findings: PBI-168, PBI-169 fixed; PBI-170 deferred (tower-lsp limitation, YAGNI)", timing: "product", status: "completed", outcome: "2/3 issues resolved, 1 deferred" },
    ] },
    { sprint: 140, improvements: [
      { action: "Flaky tests eliminated with serial_test for rust-analyzer tests", timing: "immediate", status: "completed", outcome: "373/373 tests pass consistently (10 consecutive runs verified)" },
    ] },
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
