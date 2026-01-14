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
      {
        metric: "Bridge coverage",
        target:
          "Support completion, signatureHelp, definition, typeDefinition, implementation, declaration, hover, references, document highlight, inlay hints, document link, document symbols, moniker, color presentation, rename",
      },
      {
        metric: "Modular architecture",
        target:
          "Bridge module organized with text_document/ subdirectory matching lsp_impl structure",
      },
      {
        metric: "E2E test coverage using treesitter-ls binary",
        target: "Each bridged feature has E2E test verifying end-to-end flow",
      },
    ],
  },

  product_backlog: [
    { id: "pbi-document-link", story: { role: "Lua developer editing markdown", capability: "follow links in Lua code blocks", benefit: "navigate to modules" },
      acceptance_criteria: [
        { criterion: "Bridge forwards textDocument/documentLink to downstream LS", verification: "E2E test" },
        { criterion: "Link ranges transformed to host coordinates", verification: "Unit test" },
        { criterion: "Link targets unchanged (external URIs)", verification: "Unit test" },
      ], status: "done", refinement_notes: ["Returns DocumentLink[] with range+target", "Only range needs transform"] },
    { id: "pbi-document-symbols", story: { role: "Lua developer editing markdown", capability: "see outline of symbols in Lua code block", benefit: "navigate to functions" },
      acceptance_criteria: [
        { criterion: "Bridge forwards textDocument/documentSymbol to downstream LS", verification: "E2E test" },
        { criterion: "Symbol ranges transformed to host coordinates", verification: "Unit test" },
        { criterion: "Hierarchical structure preserved", verification: "Unit test: nested children" },
      ], status: "ready", refinement_notes: ["Returns DocumentSymbol[] or SymbolInformation[]", "Recursive transform for children"] },
    { id: "pbi-inlay-hints", story: { role: "Lua developer editing markdown", capability: "see inline type hints in Lua code blocks", benefit: "understand types without hovering" },
      acceptance_criteria: [
        { criterion: "Bridge forwards textDocument/inlayHint to downstream LS", verification: "E2E test" },
        { criterion: "Hint positions transformed to host coordinates", verification: "Unit test" },
        { criterion: "Request range transformed to virtual coordinates", verification: "Unit test" },
      ], status: "ready", refinement_notes: ["Request has range, response has positions", "Bidirectional transform needed"] },
    { id: "pbi-color-presentation", story: { role: "lua/python developer editing markdown", capability: "pick and edit color values", benefit: "visual color editing" },
      acceptance_criteria: [
        { criterion: "Bridge forwards textDocument/colorPresentation to downstream LS", verification: "E2E test" },
        { criterion: "Request range transformed to virtual coordinates", verification: "Unit test" },
        { criterion: "Response textEdit ranges transformed to host coordinates", verification: "Unit test" },
      ], status: "ready", refinement_notes: ["Needs documentColor + colorPresentation handlers", "Both request and response transforms"] },
    { id: "pbi-moniker", story: { role: "lua/python developer editing markdown", capability: "get unique symbol identifiers", benefit: "cross-project navigation" },
      acceptance_criteria: [
        { criterion: "Bridge forwards textDocument/moniker to downstream LS", verification: "E2E test" },
        { criterion: "Moniker response passed through unchanged", verification: "Unit test" },
        { criterion: "Request position transformed to virtual coordinates", verification: "Unit test" },
      ], status: "ready", refinement_notes: ["Response has no position data", "Pass-through response"] },
  ],
  sprint: null,
  completed: [
    { number: 1, pbi_id: "pbi-document-highlight", goal: "Bridge textDocument/documentHighlight to downstream LS", status: "done", subtasks: [] },
    { number: 2, pbi_id: "pbi-rename", goal: "Bridge textDocument/rename with WorkspaceEdit transformation", status: "done", subtasks: [] },
    { number: 3, pbi_id: "pbi-document-link", goal: "Bridge textDocument/documentLink with range transformation to host coordinates", status: "done", subtasks: [] },
  ],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
  retrospectives: [
    { sprint: 1, improvements: [
      { action: "Review LSP spec response structure during refinement", timing: "sprint", status: "active", outcome: null },
    ] },
    { sprint: 2, improvements: [
      { action: "Continue reviewing LSP spec for dual response formats during refinement", timing: "sprint", status: "active", outcome: "Would have caught WorkspaceEdit's changes vs documentChanges formats earlier" },
      { action: "Document reusable patterns (URI filtering, coordinate transformation) for reference", timing: "immediate", status: "completed", outcome: "Pattern recognition accelerated implementation" },
    ] },
    { sprint: 3, improvements: [
      { action: "Continue using InjectionResolver::resolve_all for whole-document operations", timing: "sprint", status: "active", outcome: "Discovered pattern: whole-doc ops (documentLink, symbols) need all regions, position-based ops (hover, definition) need single region" },
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
