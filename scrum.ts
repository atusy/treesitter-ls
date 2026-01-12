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
          "Support completion, signatureHelp, codeAction, definition, typeDefinition, implementation, declaration, hover",
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
      id: "PBI-BRIDGE-IMPLEMENTATION",
      story: {
        role: "Lua developer editing markdown",
        capability: "use textDocument/implementation to find implementations of interfaces/traits in injected code blocks",
        benefit: "I can navigate to concrete implementations without leaving my markdown documentation",
      },
      acceptance_criteria: [
        {
          criterion: "textDocument/implementation requests in injection regions are forwarded to downstream LS",
          verification: "Unit test: send_implementation_request forwards to downstream with virtual URI and transformed position",
        },
        {
          criterion: "Response position coordinates are transformed from virtual to host (ADR-0015 inbound position mapping)",
          verification: "Unit test: transform_definition_response_to_host transforms Location/LocationLink ranges correctly",
        },
        {
          criterion: "Response URIs are transformed from virtual to host (ADR-0015 inbound URI transformation)",
          verification: "Unit test: Location.uri and LocationLink.targetUri are replaced with host URI",
        },
        {
          criterion: "Both Location and LocationLink response formats are handled",
          verification: "Unit tests cover: null result, single Location, Location[], LocationLink[]",
        },
        {
          criterion: "implementationProvider capability is advertised in server capabilities",
          verification: "Integration test: initialize response includes implementationProvider: true",
        },
      ],
      status: "done",
      refinement_notes: [
        "Follow exact pattern from Sprint 164/165 definition implementation",
        "Reuse transform_definition_response_to_host from protocol.rs (same Location/LocationLink format)",
        "Create text_document/implementation.rs with send_implementation_request",
        "Add build_bridge_implementation_request to protocol.rs (or generalize existing builder)",
        "Wire through bridge.rs mod declaration and lsp_impl.rs goto_implementation method",
        "Key lesson from Sprint 165: Include URI transformation from the start, not just position transformation",
      ],
    },
    {
      id: "PBI-BRIDGE-DECLARATION",
      story: {
        role: "Lua developer editing markdown",
        capability: "use textDocument/declaration to navigate to declarations in injected code blocks",
        benefit: "I can find where symbols are declared without leaving my markdown documentation",
      },
      acceptance_criteria: [
        {
          criterion: "textDocument/declaration requests in injection regions are forwarded to downstream LS",
          verification: "Unit test: send_declaration_request forwards to downstream with virtual URI and transformed position",
        },
        {
          criterion: "Response position coordinates are transformed from virtual to host (ADR-0015 inbound position mapping)",
          verification: "Unit test: transform_definition_response_to_host transforms Location/LocationLink ranges correctly",
        },
        {
          criterion: "Response URIs are transformed from virtual to host (ADR-0015 inbound URI transformation)",
          verification: "Unit test: Location.uri and LocationLink.targetUri are replaced with host URI",
        },
        {
          criterion: "Both Location and LocationLink response formats are handled",
          verification: "Unit tests cover: null result, single Location, Location[], LocationLink[]",
        },
        {
          criterion: "declarationProvider capability is advertised in server capabilities",
          verification: "Integration test: initialize response includes declarationProvider: true",
        },
      ],
      status: "ready",
      refinement_notes: [
        "Follow exact pattern from Sprint 164/165 definition implementation",
        "Reuse transform_definition_response_to_host from protocol.rs (same Location/LocationLink format)",
        "Create text_document/declaration.rs with send_declaration_request",
        "Add build_bridge_declaration_request to protocol.rs (or generalize existing builder)",
        "Wire through bridge.rs mod declaration and lsp_impl.rs goto_declaration method",
        "Key lesson from Sprint 165: Include URI transformation from the start, not just position transformation",
      ],
    },
  ],
  sprint: null,
  completed: [
    { number: 167, pbi_id: "PBI-BRIDGE-IMPLEMENTATION", goal: "Implement textDocument/implementation bridging to enable navigation to concrete implementations in injected code blocks", status: "done", subtasks: [] },
    { number: 166, pbi_id: "PBI-BRIDGE-TYPE-DEFINITION", goal: "Implement textDocument/typeDefinition bridging to enable type navigation in injected code blocks", status: "done", subtasks: [] },
    { number: 165, pbi_id: "PBI-BUGFIX-DEFINITION-URI-TRANSFORM", goal: "Fix virtual URI to host URI transformation in definition responses so users see correct document paths", status: "done", subtasks: [] },
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
    { sprint: 166, improvements: [
      { action: "Reuse pattern-compatible transformation functions across similar LSP methods", timing: "immediate", status: "completed", outcome: "Successfully reused transform_definition_response_to_host for typeDefinition - LSP spec groups goto-family methods with identical response schemas" },
    ]},
    { sprint: 165, improvements: [
      { action: "Include URI transformation from the start (not just position transformation)", timing: "immediate", status: "completed", outcome: "Documented as explicit checklist item in refinement notes to prevent Sprint 164-style oversight" },
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
