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

  // Completed PBIs: PBI-001 through PBI-112 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  // PBI-107 (remove WorkspaceType): Deferred - rust-analyzer linkedProjects too slow
  product_backlog: [
    {
      id: "PBI-111",
      story: {
        role: "Rustacean editing Markdown",
        capability: "get completion suggestions for Rust code blocks via bridge",
        benefit: "I can use familiar completion features without leaving Markdown",
      },
      acceptance_criteria: [
        {
          criterion: "src/lsp/bridge/completion.rs exists with CompletionWithNotifications type",
          verification: "grep 'CompletionWithNotifications' src/lsp/bridge/completion.rs returns matches",
        },
        {
          criterion: "LanguageServerConnection has completion_with_notifications method",
          verification: "cargo test completion_with_notifications --lib passes (unit test in connection.rs)",
        },
        {
          criterion: "textDocument/completion requests in injection regions are bridged to rust-analyzer",
          verification: "make test_nvim_file FILE=tests/test_lsp_completion.lua passes (E2E test)",
        },
        {
          criterion: "Completion results have textEdit ranges adjusted to host document positions",
          verification: "E2E test verifies completion textEdit range is in Markdown line numbers, not virtual document line numbers",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-112",
      story: {
        role: "Rustacean editing Markdown",
        capability: "see function signature help for Rust code blocks via bridge",
        benefit:
          "I can see parameter hints while calling functions without leaving Markdown",
      },
      acceptance_criteria: [
        {
          criterion:
            "src/lsp/bridge/signature_help.rs exists with SignatureHelpWithNotifications type",
          verification:
            "grep 'SignatureHelpWithNotifications' src/lsp/bridge/signature_help.rs returns matches",
        },
        {
          criterion:
            "textDocument/signatureHelp requests in injection regions are bridged",
          verification:
            "cargo test signature_help --lib passes (unit test in connection.rs)",
        },
        {
          criterion: "E2E test tests/test_lsp_signature_help.lua passes",
          verification:
            "make test_nvim_file FILE=tests/test_lsp_signature_help.lua passes",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-113",
      story: {
        role: "Rustacean editing Markdown",
        capability: "find all references for symbols in Rust code blocks via bridge",
        benefit:
          "I can navigate and understand code usage without leaving Markdown",
      },
      acceptance_criteria: [
        {
          criterion:
            "src/lsp/bridge/references.rs exists with ReferencesWithNotifications type",
          verification:
            "grep 'ReferencesWithNotifications' src/lsp/bridge/references.rs returns matches",
        },
        {
          criterion:
            "LanguageServerConnection has references_with_notifications method",
          verification:
            "cargo test references --lib passes (unit test in connection.rs)",
        },
        {
          criterion:
            "textDocument/references requests in injection regions are bridged",
          verification: "make test_nvim_file FILE=tests/test_lsp_references.lua passes",
        },
        {
          criterion:
            "Reference locations have ranges adjusted to host document positions",
          verification:
            "E2E test verifies reference ranges are in Markdown line numbers",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-114",
      story: {
        role: "Rustacean editing Markdown",
        capability: "rename symbols in Rust code blocks via bridge",
        benefit:
          "I can refactor code consistently without leaving Markdown",
      },
      acceptance_criteria: [
        {
          criterion:
            "src/lsp/bridge/rename.rs exists with RenameWithNotifications type",
          verification:
            "grep 'RenameWithNotifications' src/lsp/bridge/rename.rs returns matches",
        },
        {
          criterion:
            "LanguageServerConnection has rename_with_notifications method",
          verification:
            "cargo test rename --lib passes (unit test in connection.rs)",
        },
        {
          criterion:
            "textDocument/rename requests in injection regions are bridged",
          verification: "make test_nvim_file FILE=tests/test_lsp_rename.lua passes",
        },
        {
          criterion:
            "WorkspaceEdit TextEdit ranges adjusted to host document positions",
          verification:
            "E2E test verifies rename edit ranges are in Markdown line numbers",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-115",
      story: {
        role: "Rustacean editing Markdown",
        capability: "get code actions for Rust code blocks via bridge",
        benefit:
          "I can use quick fixes and refactorings without leaving Markdown",
      },
      acceptance_criteria: [
        {
          criterion:
            "src/lsp/bridge/code_action.rs exists with CodeActionWithNotifications type",
          verification:
            "grep 'CodeActionWithNotifications' src/lsp/bridge/code_action.rs returns matches",
        },
        {
          criterion:
            "LanguageServerConnection has code_action_with_notifications method",
          verification:
            "cargo test code_action --lib passes (unit test in connection.rs)",
        },
        {
          criterion:
            "textDocument/codeAction requests in injection regions are bridged",
          verification: "make test_nvim_file FILE=tests/test_lsp_code_action.lua passes",
        },
        {
          criterion:
            "CodeAction edit ranges and diagnostic ranges adjusted to host document positions",
          verification:
            "E2E test verifies code action edit ranges are in Markdown line numbers",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-116",
      story: {
        role: "Rustacean editing Markdown",
        capability: "format Rust code blocks via bridge",
        benefit:
          "I can keep my code blocks consistently formatted without leaving Markdown",
      },
      acceptance_criteria: [
        {
          criterion:
            "src/lsp/bridge/formatting.rs exists with FormattingWithNotifications type",
          verification:
            "grep 'FormattingWithNotifications' src/lsp/bridge/formatting.rs returns matches",
        },
        {
          criterion:
            "LanguageServerConnection has formatting_with_notifications method",
          verification:
            "cargo test formatting --lib passes (unit test in connection.rs)",
        },
        {
          criterion:
            "textDocument/formatting requests format all injection regions",
          verification: "make test_nvim_file FILE=tests/test_lsp_formatting.lua passes",
        },
        {
          criterion:
            "TextEdit ranges adjusted to host document positions",
          verification:
            "E2E test verifies formatting edit ranges are in Markdown line numbers",
        },
      ],
      status: "done",
    },
  ],

  sprint: {
    number: 93,
    pbi_id: "PBI-116",
    goal: "Bridge textDocument/formatting for injection regions",
    status: "done",
    subtasks: [],
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
      number: 93,
      pbi_id: "PBI-116",
      goal: "Bridge textDocument/formatting for injection regions",
      status: "done",
      subtasks: [],
    },
    {
      number: 92,
      pbi_id: "PBI-115",
      goal: "Bridge textDocument/codeAction for injection regions",
      status: "done",
      subtasks: [],
    },
  ],

  // Recent 2 retrospectives | Sprint 1-77: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    {
      sprint: 93,
      improvements: [
        {
          action:
            "Formatting bridging uses Obvious Implementation - formats all injection regions with TextEdit translation",
          timing: "immediate",
          status: "completed",
          outcome:
            "Formatting bridging completed by iterating all injection regions and translating TextEdit ranges to host document coordinates",
        },
      ],
    },
    {
      sprint: 92,
      improvements: [
        {
          action:
            "CodeAction bridging follows rename pattern with WorkspaceEdit translation - Obvious Implementation",
          timing: "immediate",
          status: "completed",
          outcome:
            "CodeAction bridging completed by reusing WorkspaceEdit translation pattern with additional Diagnostic range translation",
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
