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
      id: "pbi-inlay-hint-label-part-location",
      story: {
        role: "Lua developer editing markdown",
        capability: "see inlay hint label parts with clickable locations that correctly navigate to the host document",
        benefit: "I can click on type information or other hints that reference code locations and navigate to the correct position in my markdown file, not a non-existent virtual document",
      },
      acceptance_criteria: [
        {
          criterion: "Transform InlayHintLabelPart.location.range when label is an array",
          verification: "Unit test: label array with location fields has ranges transformed by region_start_line offset",
        },
        {
          criterion: "Transform InlayHintLabelPart.location.uri from virtual to host URI when same virtual document",
          verification: "Unit test: location.uri matching request's virtual URI is replaced with host URI",
        },
        {
          criterion: "Filter out InlayHintLabelPart with cross-region location.uri",
          verification: "Unit test: label parts with different virtual URI are removed from the array",
        },
        {
          criterion: "Preserve InlayHintLabelPart with real file location.uri unchanged",
          verification: "Unit test: label parts with non-virtual URI have location preserved as-is",
        },
        {
          criterion: "Preserve string labels unchanged (existing behavior)",
          verification: "Existing tests continue to pass",
        },
        {
          criterion: "Preserve InlayHintLabelPart without location field unchanged",
          verification: "Unit test: label parts with only value/tooltip/command are preserved",
        },
      ],
      status: "done",
      refinement_notes: [
        "Per LSP 3.17 spec, InlayHint.label can be string | InlayHintLabelPart[]",
        "InlayHintLabelPart has: value (required), tooltip, location, command (all optional)",
        "location field is { uri: DocumentUri, range: Range } - same as Location type",
        "Current transform_inlay_hint_item only handles position and textEdits, not label array",
        "Can reuse transform_location_uri helper for consistent URI/range transformation",
        "Need ResponseTransformContext to access host_uri and virtual_uri for URI transformation",
        "Signature change required: transform_inlay_hint_item and transform_inlay_hint_response_to_host need context parameter",
      ],
    },
  ],
  sprint: {
    number: 11,
    pbi_id: "pbi-inlay-hint-label-part-location",
    goal: "Transform InlayHintLabelPart.location for full LSP compliance",
    status: "done",
    subtasks: [
      {
        test: "N/A (structural refactoring)",
        implementation: "Change transform_inlay_hint signature from region_start_line: u32 to ResponseTransformContext",
        type: "structural",
        status: "completed",
        commits: [{ hash: "fbff5867", message: "refactor(bridge): change inlay hint transform to use ResponseTransformContext", phase: "green" }],
        notes: ["Prerequisite: enables access to host_uri and virtual_uri for URI transformation"],
      },
      {
        test: "Unit test: label array with location fields has ranges transformed by region_start_line offset",
        implementation: "Transform InlayHintLabelPart.location.range using region_start_line offset",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "a02279fb", message: "feat(bridge): transform InlayHintLabelPart.location.range to host coordinates", phase: "green" }],
        notes: ["Range transformation follows same pattern as InlayHint.position"],
      },
      {
        test: "Unit test: location.uri matching request's virtual URI is replaced with host URI",
        implementation: "Transform InlayHintLabelPart.location.uri from virtual to host when same virtual document",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "978ac6d2", message: "feat(bridge): transform InlayHintLabelPart.location using transform_location_uri", phase: "green" }],
        notes: ["Reuse transform_location_uri helper for consistent URI transformation"],
      },
      {
        test: "Unit test: label parts with different virtual URI are removed from the array",
        implementation: "Filter out InlayHintLabelPart with cross-region location.uri",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "75043c3e", message: "test(bridge): add tests for InlayHintLabelPart.location edge cases", phase: "green" }],
        notes: ["Cross-region references cannot be resolved, must be filtered", "Behavior already implemented in 978ac6d2"],
      },
      {
        test: "Unit test: label parts with non-virtual URI have location preserved as-is; label parts with only value/tooltip/command are preserved",
        implementation: "Preserve real file URIs and label parts without location field unchanged",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "75043c3e", message: "test(bridge): add tests for InlayHintLabelPart.location edge cases", phase: "green" }],
        notes: ["Real file URIs are already valid; parts without location need no transformation", "Behavior already implemented in 978ac6d2"],
      },
      {
        test: "N/A (integration point)",
        implementation: "Update pool method to build proper ResponseTransformContext",
        type: "structural",
        status: "completed",
        commits: [{ hash: "fbff5867", message: "refactor(bridge): change inlay hint transform to use ResponseTransformContext", phase: "green" }],
        notes: ["Wires up the new context parameter at the call site", "Combined with subtask 1"],
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
    { number: 10, pbi_id: "pbi-color-presentation-e2e", goal: "Add E2E test coverage for textDocument/colorPresentation", status: "done", subtasks: [] },
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
    { sprint: 10, improvements: [
      { action: "Consider batching similar PBIs (e.g., multiple E2E tests) in future sprints to reduce overhead", timing: "sprint", status: "active", outcome: null },
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
