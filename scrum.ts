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
    { id: "pbi-color-presentation", story: { role: "lua/python developer editing markdown", capability: "pick and edit color values", benefit: "visual color editing" },
      acceptance_criteria: [
        { criterion: "Bridge forwards textDocument/documentColor to downstream LS for all injection regions", verification: "E2E test" },
        { criterion: "documentColor response ranges transformed to host coordinates", verification: "Unit test" },
        { criterion: "documentColor results aggregated from multiple injection regions", verification: "Unit test" },
        { criterion: "Bridge forwards textDocument/colorPresentation to downstream LS", verification: "E2E test" },
        { criterion: "colorPresentation request range transformed to virtual coordinates", verification: "Unit test" },
        { criterion: "colorPresentation response textEdit ranges transformed to host coordinates", verification: "Unit test" },
        { criterion: "colorPresentation additionalTextEdits transformed to host coordinates", verification: "Unit test" },
      ], status: "done" },
    { id: "pbi-moniker", story: { role: "lua/python developer editing markdown", capability: "get unique symbol identifiers", benefit: "cross-project navigation" },
      acceptance_criteria: [
        { criterion: "Bridge forwards textDocument/moniker to downstream LS", verification: "E2E test" },
        { criterion: "Moniker response passed through unchanged", verification: "Unit test" },
        { criterion: "Request position transformed to virtual coordinates", verification: "Unit test" },
      ], status: "done", refinement_notes: ["Simplest pattern: position-based request + pass-through response (like signatureHelp)", "Response Moniker[] has scheme/identifier/unique/kind - no position/range data", "Follow signatureHelp implementation as template"] },
  ],
  sprint: {
    number: 7,
    pbi_id: "pbi-moniker",
    goal: "Bridge textDocument/moniker with position transformation and pass-through response",
    status: "done",
    subtasks: [
      {
        test: "Test build_bridge_moniker_request transforms position to virtual coordinates",
        implementation: "Add build_bridge_moniker_request using build_position_based_request helper",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "11ddcc1b", message: "feat(bridge): add build_bridge_moniker_request for position transformation", phase: "green" }],
        notes: ["Follow signatureHelp pattern - use build_position_based_request with 'textDocument/moniker' method"],
      },
      {
        test: "Test moniker response passes through unchanged (no position/range data to transform)",
        implementation: "Add pass-through response function (identity transform)",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "10717912", message: "feat(bridge): add transform_moniker_response_to_host pass-through", phase: "green" }],
        notes: ["Moniker[] response contains scheme/identifier/unique/kind - no coordinate data", "Verify response is returned unchanged"],
      },
      {
        test: "Test LanguageServerPool::send_moniker_request delegates to downstream LS",
        implementation: "Add send_moniker_request method to pool following signatureHelp pattern",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "c7ef8faa", message: "feat(bridge): add send_moniker_request pool method", phase: "green" }],
        notes: ["Position-based, single region lookup", "Use pass-through response transformer"],
      },
      {
        test: "E2E test verifying moniker request flows through bridge to downstream LS",
        implementation: "Wire up handler in lsp_impl and add E2E test",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "828983af", message: "feat(bridge): wire up textDocument/moniker handler with E2E test", phase: "green" }],
        notes: ["Handler wiring similar to signatureHelp", "E2E test validates end-to-end flow"],
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
  ],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
  retrospectives: [
    { sprint: 5, improvements: [
      { action: "Document bidirectional transformation pattern in ADR or architecture guide", timing: "product", status: "active", outcome: "Discovered pattern: some requests need range->virtual transformation, responses need position/textEdit->host transformation (inlayHint, colorPresentation)" },
      { action: "Distinguish hybrid pattern from pure position-based and whole-doc patterns", timing: "product", status: "active", outcome: "Hybrid pattern identified: position-based operation (single region) but with range input parameter, distinct from point-position requests" },
    ] },
    { sprint: 6, improvements: [
      { action: "Document multi-handler PBI pattern (when features require coordinated handlers)", timing: "product", status: "active", outcome: "Successfully delivered two-handler feature (documentColor + colorPresentation) by applying established patterns independently" },
      { action: "Continue leveraging pattern composition (whole-doc + hybrid patterns in single PBI)", timing: "sprint", status: "active", outcome: "documentColor used whole-doc pattern (resolve_all like documentLink), colorPresentation used hybrid pattern (range->virtual + textEdit->host like inlayHint) - zero conflicts" },
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
