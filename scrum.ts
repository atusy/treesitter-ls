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
          "Support completion, signatureHelp, references, rename, codeAction, formatting, typeDefinition, implementation, documentHighlight",
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

  // Completed PBIs: PBI-001 through PBI-122 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  // PBI-107 (remove WorkspaceType): Deferred - rust-analyzer linkedProjects too slow
  // PBI-120: Done in e600402 - bridge filter map with enabled flag (docs in docs/README.md)
  // PBI-122: Done - textDocument/typeDefinition bridge (Sprint 99)
  product_backlog: [
    {
      id: "PBI-123",
      story: {
        role: "Rustacean editing Markdown",
        capability: "use textDocument/implementation in Rust code blocks to find trait implementations",
        benefit: "I can navigate from trait methods to their concrete implementations in documentation",
      },
      acceptance_criteria: [
        {
          criterion: "implementation request in injection region bridges to language server",
          verification: "E2E test: cursor on trait method, implementation shows impl blocks",
        },
        {
          criterion: "Response positions translated from virtual to host document coordinates",
          verification: "E2E test: implementation locations are within the Markdown code block",
        },
        {
          criterion: "ServerCapabilities advertises implementationProvider",
          verification: "Unit test: initialize response includes implementationProvider: true",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-124",
      story: {
        role: "documentation author with Rust code blocks",
        capability: "see all occurrences of a symbol highlighted when cursor is on it",
        benefit: "I can quickly see where a variable or function is used within the code block",
      },
      acceptance_criteria: [
        {
          criterion: "documentHighlight request in injection region bridges to language server",
          verification: "E2E test: cursor on identifier, all occurrences highlighted",
        },
        {
          criterion: "Response positions translated from virtual to host document coordinates",
          verification: "E2E test: highlight ranges are within the Markdown code block",
        },
        {
          criterion: "ServerCapabilities advertises documentHighlightProvider",
          verification: "Unit test: initialize response includes documentHighlightProvider: true",
        },
        {
          criterion: "Highlight kinds (Read/Write/Text) preserved from language server response",
          verification: "Unit test: DocumentHighlight kind field passed through correctly",
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

  // Historical sprints (recent 2) | Sprint 1-97: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 99, pbi_id: "PBI-122", goal: "Add textDocument/typeDefinition bridge support", status: "done", subtasks: [] },
    { number: 98, pbi_id: "PBI-121", goal: "Refactor lsp_impl.rs into modular file structure", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-96: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    {
      sprint: 99,
      improvements: [
        { action: "Established reusable bridge pattern: typeDefinition implemented by copying definition.rs with minimal changes (method name, connection call)", timing: "immediate", status: "completed", outcome: "New bridge features (implementation, documentHighlight) can follow same copy-and-adapt pattern" },
        { action: "E2E tests for language servers with indexing (rust-analyzer) require explicit wait before LSP calls; added vim.uv.sleep(2000) for indexing", timing: "immediate", status: "completed", outcome: "test_lsp_type_definition.lua passes reliably" },
        { action: "Consider code generation or macro for bridge methods - definition/typeDefinition/implementation share 95%+ identical code", timing: "product", status: "active", outcome: null },
      ],
    },
    {
      sprint: 98,
      improvements: [
        { action: "Modular refactoring with *_impl delegation decomposed 3800+ line file into 10 focused text_document modules", timing: "immediate", status: "completed", outcome: "pub(crate) *_impl methods called from LanguageServer trait impl" },
        { action: "File organization by LSP category (text_document/) creates natural boundaries for future workspace/ and window/", timing: "product", status: "active", outcome: null },
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
