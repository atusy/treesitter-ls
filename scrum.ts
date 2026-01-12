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
          "Support completion, signatureHelp, codeAction, definition, typeDefinition, implementation, declaration, hover, references",
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
      id: "PBI-BRIDGE-REFERENCES",
      story: {
        role: "Lua developer editing markdown",
        capability: "find all references to a symbol in injected Lua code blocks",
        benefit: "I can navigate to all usages of variables and functions within the markdown document",
      },
      acceptance_criteria: [
        {
          criterion: "build_bridge_references_request function added to protocol.rs",
          verification: "Function builds JSON-RPC request with textDocument/references method, includeDeclaration context param, and virtual URI/position transformation",
        },
        {
          criterion: "transform_references_response_to_host function handles Location[] response",
          verification: "Unit tests verify Location array has ranges and URIs transformed to host coordinates; can reuse transform_definition_response_to_host since references uses same Location format",
        },
        {
          criterion: "send_references_request method in text_document/references.rs",
          verification: "Method follows existing pattern: get connection, send didOpen if needed, send request, wait for response with transform",
        },
        {
          criterion: "Bridge.references method wired in bridge.rs",
          verification: "Public method delegates to LanguageServerPool.send_references_request",
        },
        {
          criterion: "lsp_impl.rs calls bridge.references",
          verification: "textDocument/references handler routes to bridge for injected regions",
        },
        {
          criterion: "E2E test verifies references in injected Lua code",
          verification: "Test finds references to local variable in markdown Lua block, verifies host line coordinates",
        },
      ],
      status: "ready",
      refinement_notes: [
        "Key difference from goto-family: references returns Location[] | null (NOT LocationLink) - simpler format",
        "Request requires extra context.includeDeclaration boolean parameter",
        "Can reuse transform_definition_response_to_host since it already handles Location[] format with URI transformation",
        "Follow Sprint 165 lesson: include URI transformation from the start",
        "Pattern established in Sprints 164-168 for goto-family methods applies directly",
      ],
    },
  ],
  sprint: {
    number: 169,
    pbi_id: "PBI-BRIDGE-REFERENCES",
    goal: "Implement textDocument/references bridging to enable finding all usages of symbols in injected code blocks",
    status: "planning",
    subtasks: [
      {
        test: "Test build_bridge_references_request builds correct JSON-RPC request with textDocument/references method, virtual URI, translated position, and context.includeDeclaration parameter",
        implementation: "Add build_bridge_references_request function to protocol.rs following existing request builder pattern but including ReferenceContext with includeDeclaration boolean",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Key difference from goto-family: references request requires context.includeDeclaration param",
          "Position translation: host_position.line - region_start_line (same as definition)",
          "Virtual URI generation: reuse VirtualDocumentUri::new and to_uri_string()",
        ],
      },
      {
        test: "Test transform_definition_response_to_host handles Location[] format correctly (reuse existing tests)",
        implementation: "Verify existing transform_definition_response_to_host works for references - no new code needed since references uses same Location[] format",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "References response is Location[] | null - simpler than definition which also supports LocationLink",
          "Existing transform_definition_response_to_host already handles Location[] with range and URI transformation",
          "No new transform function needed - Sprint 165 lesson applied (URI transformation included)",
        ],
      },
      {
        test: "Test send_references_request method in text_document/references.rs sends didOpen if needed, sends request, waits for response with transform",
        implementation: "Create references.rs module with send_references_request following definition.rs pattern; add include_declaration parameter",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Follow definition.rs pattern: get_or_create_connection, should_send_didopen check, write didOpen, write request, loop read until matching id",
          "Add include_declaration: bool parameter to control context.includeDeclaration in request",
          "Reuse transform_definition_response_to_host for response transformation",
        ],
      },
      {
        test: "Test Bridge.references method delegates to LanguageServerPool.send_references_request",
        implementation: "Add references method to Bridge struct in bridge.rs that delegates to pool.send_references_request",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Follow existing pattern from definition/typeDefinition/implementation/declaration",
          "Public method accepts host coordinates and include_declaration flag",
          "Returns serde_json::Value with transformed response",
        ],
      },
      {
        test: "Test lsp_impl.rs textDocument/references handler routes to bridge.references for injected regions",
        implementation: "Wire textDocument/references in lsp_impl.rs to call bridge.references when position is in injection region",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Check if position is in injection region using existing infrastructure",
          "Extract ReferenceParams.context.include_declaration from request",
          "Call bridge.references with host URI, position, injection info, and include_declaration",
        ],
      },
      {
        test: "E2E test: find references to local variable in markdown Lua code block, verify host line coordinates in response",
        implementation: "Add E2E test in tests/ that opens markdown with Lua block, requests references on variable, verifies Location array has correct host URIs and line numbers",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Follow existing E2E test patterns with retry_for_lsp_indexing helper",
          "Test scenario: local x = 1; print(x) - references on x should return both locations",
          "Verify URI is host markdown file (not virtual), lines are host coordinates",
        ],
      },
    ],
  },
  completed: [
    { number: 168, pbi_id: "PBI-BRIDGE-DECLARATION", goal: "Implement textDocument/declaration bridging to enable navigation to symbol declarations in injected code blocks", status: "done", subtasks: [] },
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
    { sprint: 168, improvements: [
      { action: "Goto-family pattern complete: 4 methods (definition/typeDefinition/implementation/declaration) with identical pattern", timing: "immediate", status: "completed", outcome: "LSP goto-family methods share Location/LocationLink response schema - single transform_definition_response_to_host serves all. Template established for future position-based LSP methods (references, documentSymbol)" },
    ]},
    { sprint: 165, improvements: [
      { action: "Include URI transformation from the start (not just position transformation)", timing: "immediate", status: "completed", outcome: "Critical fix applied to all subsequent goto-family implementations" },
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
