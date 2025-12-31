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
        target: "Redirection module split into per-feature files for maintainability",
      },
      {
        metric: "E2E test coverage",
        target: "Each bridged feature has E2E test verifying end-to-end flow",
      },
    ],
  },

  // Completed PBIs: PBI-001 through PBI-109 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  // PBI-107 (remove WorkspaceType): Deferred - rust-analyzer linkedProjects approach too slow for E2E tests
  product_backlog: [
    {
      id: "PBI-109",
      story: {
        role: "Rustacean editing Markdown",
        capability: "see the Product Goal updated to reflect expanded bridge vision",
        benefit:
          "the team has clear direction on expanding bridge support beyond go-to-definition",
      },
      acceptance_criteria: [
        {
          criterion: "Product Goal statement changed from go-to-definition focus to broader bridge expansion",
          verification: "Read scrum.ts and verify product_goal.statement mentions expanded bridge support",
        },
        {
          criterion: "Success metrics updated to cover new bridged features",
          verification: "Verify success_metrics list includes bridge coverage, modular architecture, and E2E tests",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-110",
      story: {
        role: "developer editing Lua files",
        capability: "have redirection.rs refactored into a module structure",
        benefit:
          "each bridged LSP feature is in its own file making maintenance and extension easier",
      },
      acceptance_criteria: [
        {
          criterion: "src/lsp/redirection/ directory created (modern module style with redirection.rs)",
          verification: "ls src/lsp/redirection/ shows submodules and redirection.rs exists in parent",
        },
        {
          criterion: "definition.rs contains goto_definition related code",
          verification: "grep GotoDefinitionWithNotifications src/lsp/redirection/definition.rs returns matches",
        },
        {
          criterion: "hover.rs contains hover related code",
          verification: "grep HoverWithNotifications src/lsp/redirection/hover.rs returns matches",
        },
        {
          criterion: "Existing tests pass without modification",
          verification: "make test passes (structural change only, no behavioral change)",
        },
      ],
      status: "done",
    },
    {
      id: "PBI-111",
      story: {
        role: "Rustacean editing Markdown",
        capability: "get completion suggestions for Rust code blocks via bridge",
        benefit: "I can use familiar completion features without leaving Markdown",
      },
      acceptance_criteria: [
        {
          criterion: "src/lsp/redirection/completion.rs exists with CompletionWithNotifications type",
          verification: "grep 'CompletionWithNotifications' src/lsp/redirection/completion.rs returns matches",
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
      status: "ready",
    },
    {
      id: "PBI-112",
      story: {
        role: "Rustacean editing Markdown",
        capability: "see function signature help for Rust code blocks via bridge",
        benefit: "I can see parameter hints while calling functions in code blocks",
      },
      acceptance_criteria: [
        {
          criterion: "textDocument/signatureHelp requests in injection regions are bridged",
          verification: "E2E test sends signatureHelp request in Rust code block and receives signature info",
        },
        {
          criterion: "Signature help shows correct parameter documentation",
          verification: "E2E test verifies signature contains expected parameter names",
        },
      ],
      status: "draft",
    },
    {
      id: "PBI-113",
      story: {
        role: "Rustacean editing Markdown",
        capability: "find all references in Rust code blocks via bridge",
        benefit: "I can see where symbols are used within the code block",
      },
      acceptance_criteria: [
        {
          criterion: "textDocument/references requests in injection regions are bridged",
          verification: "E2E test sends references request in Rust code block and receives location list",
        },
        {
          criterion: "Reference locations are adjusted to original document positions",
          verification: "E2E test verifies reference locations point to correct lines in Markdown",
        },
      ],
      status: "draft",
    },
    {
      id: "PBI-114",
      story: {
        role: "Rustacean editing Markdown",
        capability: "rename symbols in Rust code blocks via bridge",
        benefit: "I can refactor variable names within code blocks safely",
      },
      acceptance_criteria: [
        {
          criterion: "textDocument/rename requests in injection regions are bridged",
          verification: "E2E test sends rename request in Rust code block and receives workspace edit",
        },
        {
          criterion: "Rename edits have positions adjusted to original document",
          verification: "E2E test verifies edit ranges point to correct positions in Markdown",
        },
      ],
      status: "draft",
    },
    {
      id: "PBI-115",
      story: {
        role: "Rustacean editing Markdown",
        capability: "access code actions in Rust code blocks via bridge",
        benefit: "I can use quick fixes and refactorings in code blocks",
      },
      acceptance_criteria: [
        {
          criterion: "textDocument/codeAction requests in injection regions are bridged",
          verification: "E2E test sends codeAction request in Rust code block and receives action list",
        },
        {
          criterion: "Code action edits have positions adjusted to original document",
          verification: "E2E test verifies any workspace edits point to correct Markdown positions",
        },
      ],
      status: "draft",
    },
    {
      id: "PBI-116",
      story: {
        role: "Rustacean editing Markdown",
        capability: "format Rust code blocks via bridge",
        benefit: "I can keep code blocks properly formatted using rustfmt",
      },
      acceptance_criteria: [
        {
          criterion: "textDocument/formatting requests for injection regions are bridged",
          verification: "E2E test sends formatting request targeting Rust code block and receives edits",
        },
        {
          criterion: "Formatted code replaces only the code block content",
          verification: "E2E test verifies formatting edits are scoped to code block range",
        },
      ],
      status: "draft",
    },
  ],

  sprint: {
    number: 87,
    pbi_id: "PBI-110",
    goal: "Refactor redirection.rs into module structure for maintainability",
    status: "done",
    subtasks: [
      {
        test: "Verify module structure exists with required files",
        implementation: "Split redirection.rs into redirection/ module with cleanup.rs, connection.rs, definition.rs, hover.rs, pool.rs, workspace.rs",
        type: "structural",
        status: "completed",
        commits: [],
        notes: [
          "Used modern Rust module style: redirection.rs + redirection/ directory (not mod.rs)",
          "All 308 tests pass - structural change only, no behavioral changes",
          "definition.rs contains GotoDefinitionWithNotifications, hover.rs contains HoverWithNotifications",
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
      number: 86,
      pbi_id: "PBI-109",
      goal: "Update Product Goal statement to reflect expanded bridge vision",
      status: "done",
      subtasks: [],
    },
    {
      number: 85,
      pbi_id: "PBI-108",
      goal:
        "Add per-host language bridge filter configuration to control which injection languages are bridged",
      status: "done",
      subtasks: [],
    },
  ],

  // Recent 2 retrospectives | Sprint 1-77: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    {
      sprint: 87,
      improvements: [
        {
          action:
            "Use modern Rust module style (module.rs + module/ directory) instead of mod.rs for better editor navigation",
          timing: "immediate",
          status: "completed",
          outcome:
            "Module files clearly named (redirection.rs, cleanup.rs, etc.) instead of multiple mod.rs tabs in editor",
        },
        {
          action:
            "Structural refactoring with extensive test suite validates no behavioral changes - 308 tests provide confidence for safe module splitting",
          timing: "immediate",
          status: "completed",
          outcome:
            "Refactored 2000+ line file into 6 focused modules while all tests continued to pass",
        },
      ],
    },
    {
      sprint: 86,
      improvements: [
        {
          action:
            "Verification-only sprints (checking existing state) complete quickly when criteria are already satisfied",
          timing: "immediate",
          status: "completed",
          outcome:
            "Sprint 86 completed immediately by confirming Product Goal already reflected expanded bridge vision",
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
