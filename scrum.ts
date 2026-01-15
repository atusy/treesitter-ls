// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Lua developer editing markdown",
  "lua/python developer editing markdown",
] as const satisfies readonly string[]; // Must have at least one role. Avoid generic roles like "user" or "admin". Remove obsolete roles freely.

const scrum: ScrumDashboard = {
  product_goal: {
    statement: "Improve LSP feature coverage via bridge",
    success_metrics: [
      { metric: "Bridge coverage", target: "Support completion, signatureHelp, definition, typeDefinition, implementation, declaration, hover, references, document highlight, inlay hints, document link, document symbols, moniker, color presentation, rename" },
      { metric: "Modular architecture", target: "Bridge module organized with text_document/ subdirectory matching lsp_impl structure" },
      { metric: "E2E test coverage using treesitter-ls binary", target: "Each bridged feature has E2E test verifying end-to-end flow" },
    ],
  },
  product_backlog: [
    {
      id: "pbi-symbol-info-uri-fix",
      story: {
        role: "Lua developer editing markdown",
        capability: "document symbols to work with SymbolInformation responses",
        benefit: "older language servers are supported",
      },
      acceptance_criteria: [
        {
          criterion: "SymbolInformation.location.uri transformed to host URI",
          verification: "Unit test verifies URI transformation from virtual to host",
        },
        {
          criterion: "SymbolInformation.location.range transformed to host coordinates",
          verification: "Verify existing range transformation works correctly (already implemented)",
        },
      ],
      status: "done",
    },
    {
      id: "pbi-document-color-e2e",
      story: {
        role: "lua/python developer editing markdown",
        capability: "document color feature to be E2E tested",
        benefit: "I have confidence in the feature",
      },
      acceptance_criteria: [
        {
          criterion: "E2E test verifies documentColor capability is advertised",
          verification: "Test checks server capabilities include documentColor",
        },
        {
          criterion: "E2E test verifies request handling",
          verification: "Test sends documentColor request and verifies response (even if empty)",
        },
      ],
      status: "done",
    },
    {
      id: "pbi-color-presentation-e2e",
      story: {
        role: "lua/python developer editing markdown",
        capability: "color presentation feature to be E2E tested",
        benefit: "I have confidence in the feature",
      },
      acceptance_criteria: [
        {
          criterion: "E2E test verifies colorPresentation capability is advertised",
          verification: "Test checks server capabilities include colorPresentation",
        },
        {
          criterion: "E2E test verifies request handling",
          verification: "Test sends colorPresentation request and verifies response (even if empty)",
        },
      ],
      status: "ready",
      refinement_notes: [
        "Missing E2E test - No tests/e2e_lsp_lua_color_presentation.rs exists",
        "Create E2E test file following existing patterns in tests/e2e_lsp_lua_*.rs",
      ],
    },
  ],
  sprint: {
    number: 10,
    pbi_id: "pbi-color-presentation-e2e",
    goal: "Add E2E test coverage for textDocument/colorPresentation",
    status: "in_progress",
    subtasks: [
      {
        test: "E2E test verifies colorPresentation capability is advertised in server capabilities",
        implementation: "Create tests/e2e_lsp_lua_color_presentation.rs with capability check test",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Follow pattern from e2e_lsp_lua_document_color.rs", "Check for colorProvider in capabilities"],
      },
      {
        test: "E2E test verifies colorPresentation request is handled without error",
        implementation: "Add colorPresentation request test with mock color/range input",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "colorPresentation requires color and range as input (from documentColor)",
          "Use mock color values since lua-ls may not return actual colors",
          "Verify response structure (array of ColorPresentation or empty)",
        ],
      },
    ],
  },
  completed: [
    { number: 1, pbi_id: "pbi-document-highlight", goal: "Bridge textDocument/documentHighlight to downstream LS", status: "done", subtasks: [] },
    { number: 2, pbi_id: "pbi-rename", goal: "Bridge textDocument/rename with WorkspaceEdit transformation", status: "done", subtasks: [] },
    { number: 3, pbi_id: "pbi-document-link", goal: "Bridge textDocument/documentLink with range transformation to host coordinates", status: "done", subtasks: [] },
    { number: 4, pbi_id: "pbi-document-symbols", goal: "Bridge textDocument/documentSymbol to downstream LS with coordinate transformation", status: "done", subtasks: [] },
    { number: 5, pbi_id: "pbi-inlay-hints", goal: "Bridge textDocument/inlayHint with bidirectional coordinate transformation", status: "done", subtasks: [] },
    { number: 6, pbi_id: "pbi-color-presentation", goal: "Bridge textDocument/documentColor and textDocument/colorPresentation with coordinate transformation", status: "done", subtasks: [] },
    { number: 7, pbi_id: "pbi-moniker", goal: "Bridge textDocument/moniker with position transformation and pass-through response", status: "done", subtasks: [] },
    { number: 8, pbi_id: "pbi-symbol-info-uri-fix", goal: "Fix SymbolInformation URI transformation for LSP compliance", status: "done", subtasks: [] },
    { number: 9, pbi_id: "pbi-document-color-e2e", goal: "Add E2E test coverage for textDocument/documentColor", status: "done", subtasks: [] },
  ],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
      { name: "E2E test exists for bridged features (test infrastructure even if downstream LS returns no data)", run: "verify tests/e2e_lsp_lua_*.rs exists for feature" },
    ],
  },
  retrospectives: [
    { sprint: 9, improvements: [
      { action: "Update definition of done to include E2E tests as mandatory for all bridged features", timing: "immediate", status: "completed", outcome: "Added E2E test requirement to definition_of_done checks" },
      { action: "Document pattern for testing bridged features where downstream LS may not return data (verify infrastructure without requiring actual results)", timing: "immediate", status: "completed", outcome: "Added 'Testing Patterns' section to CLAUDE.md with example pattern" },
    ] },
    { sprint: 8, improvements: [
      { action: "Establish multi-perspective review practice to catch LSP compliance issues earlier", timing: "sprint", status: "active", outcome: null },
      { action: "Ensure dual response formats (DocumentSymbol[] vs SymbolInformation[]) are equally tested for all bridged features", timing: "product", status: "active", outcome: null },
    ] },
  ],
};

// Type Definitions (DO NOT MODIFY) =============================================
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
  refinement_notes?: string[];
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
