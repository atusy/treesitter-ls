// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Lua developer editing markdown",
  "lua/python developer editing markdown",
] as const satisfies readonly string[]; // Must have at least one role. Avoid generic roles like "user" or "admin". Remove obsolete roles freely.

const scrum: ScrumDashboard = {
  product_goal: {
    statement:
      "Implement LSP bridge to support essential language server features indirectly through bridging (ADR-0013, 0014, 0015, 0016, 0017, 0018)",
    success_metrics: [
      {
        metric: "ADR alignment",
        target:
          "Must align with Phase 1 of ADR-0013, 0014, 0015, 0016, 0017, 0018 in @docs/adr",
      },
      {
        metric: "Bridge coverage",
        target:
          "Support completion, signatureHelp, codeAction, definition, hover",
      },
      {
        metric: "Modular architecture",
        target:
          "Bridge module organized with text_document/ subdirectory matching lsp_impl structure",
      },
      {
        metric: "E2E test coverage",
        target: "Each bridged feature has E2E test verifying end-to-end flow",
      },
    ],
  },

  product_backlog: [
    {
      id: "PBI-BUGFIX-DEFINITION-URI-TRANSFORM",
      story: {
        role: "Lua developer editing markdown",
        capability: "go to definition within injection regions and see the correct host document URI",
        benefit: "I can navigate to definitions without seeing confusing virtual document paths like /.treesitter-ls/41ca8324e35c1615/lua-0.lua",
      },
      acceptance_criteria: [
        {
          criterion: "Location responses have uri field transformed from virtual URI to host URI",
          verification: "Unit test: transform_definition_response_to_host replaces virtual URI with host URI in Location format",
        },
        {
          criterion: "LocationLink responses have targetUri field transformed from virtual URI to host URI",
          verification: "Unit test: transform_definition_response_to_host replaces virtual URI with host URI in LocationLink format",
        },
        {
          criterion: "transform_definition_response_to_host receives host_uri parameter",
          verification: "Function signature updated to accept host_uri; callers pass host_uri through",
        },
        {
          criterion: "Existing line number transformations continue to work",
          verification: "Existing unit tests for range transformation still pass",
        },
      ],
      status: "ready" as PBIStatus,
      refinement_notes: [
        "Bug: Definition response shows virtual document URIs instead of host document URIs",
        "Root cause: transform_definition_response_to_host transforms line numbers but not URIs",
        "ADR-0015/0016 specify 'Inbound: Transform virtual URI -> host URI' but this was not implemented",
        "Affected fields: Location.uri, LocationLink.targetUri",
        "NOT affected: LocationLink.originSelectionRange (already in host coordinates)",
        "Implementation: Add host_uri parameter to transform function and replace matching virtual URIs",
        "Pattern: Virtual URIs match pattern file:///.treesitter-ls/{hash}/{region}.{ext}",
        "Scope: Only definition.rs and protocol.rs need changes; hover/completion don't return URIs",
      ],
    },
  ],
  sprint: {
    number: 165,
    pbi_id: "PBI-BUGFIX-DEFINITION-URI-TRANSFORM",
    goal: "Fix virtual URI to host URI transformation in definition responses so users see correct document paths",
    status: "planning" as SprintStatus,
    subtasks: [
      {
        test: "Unit test: transform_definition_response_to_host replaces virtual URI with host URI in Location.uri field",
        implementation: "Update transform_definition_response_to_host signature to accept host_uri parameter and transform Location.uri",
        type: "behavioral" as SubtaskType,
        status: "pending" as SubtaskStatus,
        commits: [],
        notes: ["Location format: { uri, range } - need to replace uri when it matches virtual pattern"],
      },
      {
        test: "Unit test: transform_definition_response_to_host replaces virtual URI with host URI in LocationLink.targetUri field",
        implementation: "Extend transform_definition_item to also transform targetUri field in LocationLink format",
        type: "behavioral" as SubtaskType,
        status: "pending" as SubtaskStatus,
        commits: [],
        notes: ["LocationLink format: { targetUri, targetRange, targetSelectionRange, originSelectionRange }"],
      },
      {
        test: "Verify existing range transformation tests still pass",
        implementation: "Update test assertions to include host_uri parameter in all transform_definition_response_to_host calls",
        type: "behavioral" as SubtaskType,
        status: "pending" as SubtaskStatus,
        commits: [],
        notes: ["Existing tests: definition_response_transforms_location_array_ranges, definition_response_transforms_single_location, definition_response_transforms_location_link_array, definition_response_with_null_result_passes_through"],
      },
      {
        test: "Integration: definition.rs caller passes host_uri to transform function",
        implementation: "Update send_definition_request to pass host_uri to transform_definition_response_to_host",
        type: "behavioral" as SubtaskType,
        status: "pending" as SubtaskStatus,
        commits: [],
        notes: ["definition.rs line 88: transform_definition_response_to_host(msg, region_start_line) needs host_uri"],
      },
    ],
  },
  completed: [
    { number: 164, pbi_id: "PBI-BRIDGE-DEFINITION", goal: "Implement textDocument/definition bridging with coordinate transformation for Location and LocationLink response formats", status: "done", subtasks: [] },
    { number: 163, pbi_id: "PBI-REFACTOR-DIDCHANGE-MODULE", goal: "Extract didChange logic to text_document/did_change.rs module for consistent architecture", status: "done", subtasks: [] },
    { number: 162, pbi_id: "PBI-REFACTOR-DIDCLOSE-MODULE", goal: "Extract didClose logic to text_document/did_close.rs module for consistent architecture", status: "done", subtasks: [] },
    { number: 161, pbi_id: "PBI-DIDCHANGE-FORWARDING", goal: "Forward didChange notifications from host documents to opened virtual documents", status: "done", subtasks: [] },
  ],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
  retrospectives: [
    { sprint: 164, improvements: [
      { action: "Create ADR for dual-format LSP response patterns (Location vs LocationLink, CompletionItem vs CompletionList)", timing: "product", status: "active", outcome: null },
      { action: "Extract helper functions early when implementing dual-format transformations (pattern: transform_definition_item for handling Location/LocationLink)", timing: "immediate", status: "completed", outcome: "Helper function transform_definition_item cleanly separates single-item transformation logic from array/object handling - apply this pattern in future response transformations from the start of TDD cycle" },
      { action: "Add checklist item for E2E tests: verify both response format variants when LSP spec allows alternatives", timing: "sprint", status: "active", outcome: null },
    ]},
    { sprint: 163, improvements: [
      { action: "Paired accessors with read vs consume semantics", timing: "immediate", status: "completed", outcome: "get_host_virtual_docs (read) vs remove_host_virtual_docs (consume) - clear semantic naming enables correct usage for different module needs (didChange reads, didClose consumes)" },
    ]},
    { sprint: 162, improvements: [
      { action: "Use pub(super) accessor pattern for module extraction", timing: "immediate", status: "completed", outcome: "Accessor methods enable submodules to access private fields while maintaining encapsulation - pattern reused in Sprint 163" },
    ]},
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
