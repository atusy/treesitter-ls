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
      status: "ready",
      refinement_notes: [
        "CRITICAL - LSP Compliance issue",
        "In response.rs:325-331, when downstream LS returns SymbolInformation[] format, location.uri contains virtual URI but is NOT transformed to host URI",
        "Fix: Transform location.uri from virtual to host URI, similar to how definition responses handle this",
      ],
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
      status: "ready",
      refinement_notes: [
        "Missing E2E test - No tests/e2e_lsp_lua_document_color.rs exists",
        "Create E2E test file following existing patterns in tests/e2e_lsp_lua_*.rs",
      ],
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
    number: 8,
    pbi_id: "pbi-symbol-info-uri-fix",
    goal: "Fix SymbolInformation URI transformation for LSP compliance",
    status: "in_progress",
    subtasks: [
      {
        test: "Add unit test for SymbolInformation.location.uri transformation with virtual URI",
        implementation: "Create test verifying virtual URI is transformed to host URI",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Test should use virtual URI format (treesitter-ls://host_uri/lang/region_id)",
          "Test should verify URI is transformed to host URI after transformation",
          "Follow existing pattern in document_symbol_response_transforms_symbol_information_location_range test",
        ],
      },
      {
        test: "Update transform_document_symbol_response_to_host signature to use ResponseTransformContext",
        implementation: "Change function signature from (response, region_start_line) to (response, &ResponseTransformContext)",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "ResponseTransformContext contains: request_virtual_uri, request_host_uri, request_region_start_line",
          "Update document_symbol.rs call site to create and pass ResponseTransformContext",
          "Update all existing tests to use new signature",
        ],
      },
      {
        test: "Implement URI transformation in SymbolInformation handling",
        implementation: "In transform_document_symbol_item, transform location.uri using same pattern as transform_location_uri",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Handle three cases: (1) real file URI - preserve, (2) same virtual URI - transform, (3) different virtual URI - filter",
          "Call transform_location_uri helper from transform_document_symbol_item for SymbolInformation case",
          "Ensure range transformation still works after URI transformation",
        ],
      },
      {
        test: "Verify existing range transformation still works with new context-based signature",
        implementation: "Run existing tests to confirm range transformation is preserved",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Existing tests: document_symbol_response_transforms_symbol_information_location_range",
          "Verify DocumentSymbol format (range, selectionRange, children) still works",
          "Verify null and empty array edge cases still pass through correctly",
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
  ],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
  retrospectives: [
    { sprint: 7, improvements: [
      { action: "Complete request-side pattern documentation in request.rs", timing: "product", status: "active", outcome: "Response patterns documented in response.rs header. Request patterns (build_position_based_request, build_range_based_request) exist but need similar module-level documentation for developer guidance." },
      { action: "Bridge coverage 100% achieved", timing: "product", status: "completed", outcome: "Sprints 1-7 delivered all 16 LSP features. Pattern taxonomy: position-based (signatureHelp, moniker), hybrid (inlayHint, colorPresentation), whole-doc (documentLink, documentColor), context-based (definition, rename)." },
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
