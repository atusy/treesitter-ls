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
  role: string;
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

// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const scrum: ScrumDashboard = {
  product_goal: {
    statement:
      "Provide production ready language server bridge for go to definition",
    success_metrics: [
      {
        metric: "ADR alignment",
        target:
          "AI review confirms the implementation aligns with ADR-0006, ADR-0007, ADR-0008",
      },
    ],
  },

  // Completed PBIs: PBI-001 through PBI-087 | History: git log -- scrum.yaml, scrum.ts
  product_backlog: [
    {
      id: "PBI-088",
      story: {
        role: "user",
        capability: "configure which language servers handle which injection languages",
        benefit: "I can use rust-analyzer, pyright, gopls for code blocks in Markdown",
      },
      acceptance_criteria: [
        { criterion: "TreeSitterSettings includes bridge.servers config", verification: "cargo test test_bridge_config_parsing" },
        { criterion: "initializationOptions passed to server's initialize", verification: "Integration test verifies server receives options" },
        { criterion: "Only configured servers spawned (security)", verification: "Unknown language does not spawn process" },
        { criterion: "Graceful fallback for unconfigured languages", verification: "Semantic tokens work without bridge config" },
      ],
      status: "draft",
    },
    {
      id: "PBI-089",
      story: {
        role: "user",
        capability: "see type info and docs when hovering over symbols in code blocks",
        benefit: "I understand APIs without leaving my Markdown documentation",
      },
      acceptance_criteria: [
        { criterion: "textDocument/hover forwarded to rust-analyzer for Rust injections", verification: "cargo test test_hover_rust_injection" },
        { criterion: "Response range translated from virtual to host coordinates", verification: "cargo test test_hover_range_translation" },
        { criterion: "Returns null when position outside injection region", verification: "cargo test test_hover_outside_injection_returns_null" },
        { criterion: "E2E: Neovim shows hover info in Markdown Rust block", verification: "make test_nvim_file FILE=tests/test_lsp_hover.lua" },
      ],
      status: "done",
    },
    {
      id: "PBI-090",
      story: {
        role: "user",
        capability: "see function signature help while typing in code blocks",
        benefit: "I know what arguments are expected",
      },
      acceptance_criteria: [
        { criterion: "textDocument/signatureHelp forwarded", verification: "E2E: signature shown on '('" },
        { criterion: "Response passed through unchanged", verification: "Unit: no position mapping needed" },
      ],
      status: "draft",
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

  // Historical sprints (recent 3) | Sprint 1-67: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 70, pbi_id: "PBI-089", goal: "Users see type info when hovering over Rust symbols in Markdown code blocks", status: "done", subtasks: [] },
    { number: 69, pbi_id: "PBI-087", goal: "ServerPool for connection reuse (<200ms latency)", status: "done", subtasks: [] },
    { number: 68, pbi_id: "PBI-086", goal: "Go-to-definition in Markdown Rust code blocks", status: "done", subtasks: [] },
  ],

  // Recent 3 retrospectives | Sprint 68-70: git log -- scrum.ts
  retrospectives: [
    {
      sprint: 70,
      improvements: [
        { action: "Obvious Implementation is valid TDD when pattern is clear", timing: "immediate", status: "completed", outcome: "Added guidance to CLAUDE.md TDD section" },
        { action: "ADR-0008 Priority 1 (hover) was accurate - low complexity, high value", timing: "immediate", status: "completed", outcome: "Single commit for complete feature" },
        { action: "Pre-existing E2E failures need documentation", timing: "sprint", status: "active", outcome: null },
        { action: "Fix 3 failing E2E tests (test_lsp_select x2, test_lsp_semantic x1)", timing: "product", status: "active", outcome: null },
      ],
    },
    {
      sprint: 69,
      improvements: [
        { action: "DashMap simplifies thread-safe primitives", timing: "immediate", status: "completed", outcome: "Minimal boilerplate for ServerPool" },
        { action: "Mock server (cat) enables fast unit tests", timing: "immediate", status: "completed", outcome: "Tests run <1s" },
        { action: "cleanup_idle() needs timer wiring", timing: "product", status: "active", outcome: null },
        { action: "ServerPool not yet in lsp_impl.rs", timing: "sprint", status: "active", outcome: null },
        { action: "get_or_spawn() handles crashed recovery", timing: "immediate", status: "completed", outcome: "Atomic respawn on is_alive() failure" },
      ],
    },
    {
      sprint: 68,
      improvements: [
        { action: "E2E temp files need vim.cmd.write()", timing: "immediate", status: "completed", outcome: "Child neovim sees persisted file" },
        { action: "LSP ops need explicit wait (~2s for rust-analyzer)", timing: "immediate", status: "completed", outcome: "Added vim.wait(2000)" },
        { action: "PoC sync subprocess needs production refactor", timing: "product", status: "active", outcome: "Consider pooling, async spawn" },
      ],
    },
    {
      sprint: 67,
      improvements: [
        { action: "Simple is_injection param fixed root cause", timing: "immediate", status: "completed", outcome: "Avoided tokio::spawn complexity" },
        { action: "E2E auto-install tests are invasive", timing: "product", status: "active", outcome: "Need isolated parser directories" },
        { action: "Clear symptom descriptions enable fast analysis", timing: "immediate", status: "completed", outcome: "User report mapped to code flow" },
      ],
    },
  ],
};

// JSON output (deno run scrum.ts | jq for queries)
console.log(JSON.stringify(scrum, null, 2));
