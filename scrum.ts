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
    { id: "pbi-color-presentation", story: { role: "lua/python developer editing markdown", capability: "pick and edit color values", benefit: "visual color editing" },
      acceptance_criteria: [
        // documentColor: whole-document operation (like documentLink)
        { criterion: "Bridge forwards textDocument/documentColor to downstream LS for all injection regions", verification: "E2E test" },
        { criterion: "documentColor response ranges transformed to host coordinates", verification: "Unit test" },
        { criterion: "documentColor results aggregated from multiple injection regions", verification: "Unit test" },
        // colorPresentation: hybrid pattern - range input with textEdit response (like inlayHint)
        { criterion: "Bridge forwards textDocument/colorPresentation to downstream LS", verification: "E2E test" },
        { criterion: "colorPresentation request range transformed to virtual coordinates", verification: "Unit test" },
        { criterion: "colorPresentation response textEdit ranges transformed to host coordinates", verification: "Unit test" },
        { criterion: "colorPresentation additionalTextEdits transformed to host coordinates", verification: "Unit test" },
      ], status: "ready", refinement_notes: ["Two-part feature: documentColor (whole-doc) + colorPresentation (hybrid)", "documentColor uses resolve_all pattern like documentLink", "colorPresentation uses range->virtual request + textEdit->host response like inlayHint", "ColorInformation[] response contains range+color for each color found", "ColorPresentation[] response contains label + optional textEdit + optional additionalTextEdits"] },
    { id: "pbi-moniker", story: { role: "lua/python developer editing markdown", capability: "get unique symbol identifiers", benefit: "cross-project navigation" },
      acceptance_criteria: [
        { criterion: "Bridge forwards textDocument/moniker to downstream LS", verification: "E2E test" },
        { criterion: "Moniker response passed through unchanged", verification: "Unit test" },
        { criterion: "Request position transformed to virtual coordinates", verification: "Unit test" },
      ], status: "ready", refinement_notes: ["Response has no position data", "Pass-through response"] },
  ],
  sprint: {
    number: 6,
    pbi_id: "pbi-color-presentation",
    goal: "Bridge textDocument/documentColor and textDocument/colorPresentation with coordinate transformation",
    status: "in_progress",
    subtasks: [
      // documentColor: whole-document pattern (like documentLink)
      { test: "transform_color_information converts virtual ranges to host coordinates", implementation: "Add transform_color_information function in document_color.rs", type: "behavioral", status: "completed", commits: [], notes: ["ColorInformation[] contains range+color", "Uses host_document_from_virtual_position for range transformation"] },
      { test: "resolve_document_color aggregates results from all injection regions", implementation: "Add resolve_document_color to LanguageServerPool using resolve_all pattern", type: "behavioral", status: "pending", commits: [], notes: ["Whole-document operation like documentLink", "Aggregates ColorInformation[] from all regions"] },
      { test: "E2E documentColor returns color locations from injection region", implementation: "Wire documentColor handler in lsp_impl", type: "behavioral", status: "pending", commits: [], notes: ["Handler wiring + E2E verification"] },
      // colorPresentation: hybrid pattern (range input with textEdit response)
      { test: "build_color_presentation_params transforms range to virtual coordinates", implementation: "Add build_color_presentation_params function in color_presentation.rs", type: "behavioral", status: "pending", commits: [], notes: ["Hybrid pattern: range input needs virtual transformation", "Similar to inlayHint request builder"] },
      { test: "transform_color_presentation converts textEdit and additionalTextEdits to host coordinates", implementation: "Add transform_color_presentation function handling both edit types", type: "behavioral", status: "pending", commits: [], notes: ["ColorPresentation has optional textEdit + optional additionalTextEdits", "Both need range->host transformation"] },
      { test: "resolve_color_presentation forwards to correct region based on range", implementation: "Add resolve_color_presentation to LanguageServerPool using hybrid pattern", type: "behavioral", status: "pending", commits: [], notes: ["Position-based operation (single region)", "Range input determines which injection region"] },
      { test: "E2E colorPresentation returns edit suggestions for color", implementation: "Wire colorPresentation handler in lsp_impl + E2E test", type: "behavioral", status: "pending", commits: [], notes: ["Handler wiring + E2E verification", "Verify textEdit ranges are correctly transformed"] },
    ],
  },
  completed: [
    { number: 1, pbi_id: "pbi-document-highlight", goal: "Bridge textDocument/documentHighlight to downstream LS", status: "done", subtasks: [] },
    { number: 2, pbi_id: "pbi-rename", goal: "Bridge textDocument/rename with WorkspaceEdit transformation", status: "done", subtasks: [] },
    { number: 3, pbi_id: "pbi-document-link", goal: "Bridge textDocument/documentLink with range transformation to host coordinates", status: "done", subtasks: [] },
    { number: 4, pbi_id: "pbi-document-symbols", goal: "Bridge textDocument/documentSymbol to downstream LS with coordinate transformation", status: "done", subtasks: [] },
    { number: 5, pbi_id: "pbi-inlay-hints", goal: "Bridge textDocument/inlayHint with bidirectional coordinate transformation", status: "done", subtasks: [] },
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
    ] },
    { sprint: 3, improvements: [
      { action: "Continue using InjectionResolver::resolve_all for whole-document operations", timing: "sprint", status: "active", outcome: "Discovered pattern: whole-doc ops (documentLink, symbols) need all regions, position-based ops (hover, definition) need single region" },
    ] },
    { sprint: 4, improvements: [
      { action: "Proactively implement dual response formats when LSP spec shows both options", timing: "sprint", status: "active", outcome: "Both DocumentSymbol[] and SymbolInformation[] formats implemented upfront, eliminating need for additional subtasks" },
      { action: "Handle recursive structures (DocumentSymbol.children) during initial implementation", timing: "sprint", status: "active", outcome: "Recursive transformation implemented with initial test, avoiding regression risk" },
      { action: "Continue leveraging established patterns (whole-doc ops, simple transformers)", timing: "sprint", status: "active", outcome: "Sprint executed cleanly with zero blockers by reusing document_link.rs pattern" },
    ] },
    { sprint: 5, improvements: [
      { action: "Document bidirectional transformation pattern in ADR or architecture guide", timing: "product", status: "active", outcome: "Discovered pattern: some requests need range->virtual transformation, responses need position/textEdit->host transformation (inlayHint, colorPresentation)" },
      { action: "Distinguish hybrid pattern from pure position-based and whole-doc patterns", timing: "product", status: "active", outcome: "Hybrid pattern identified: position-based operation (single region) but with range input parameter, distinct from point-position requests" },
      { action: "Continue handling optional nested transformations (textEdits, labelPart.location)", timing: "sprint", status: "active", outcome: "Optional textEdits handled correctly; labelPart.location deferred as rare cross-doc case" },
      { action: "Continue TDD cycle execution with small, focused subtasks", timing: "sprint", status: "active", outcome: "5 subtasks completed cleanly: request builder, position transform, textEdits transform, pool method, E2E test - zero blockers" },
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
