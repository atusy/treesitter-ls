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
          "Support completion, signatureHelp, codeAction, definition, typeDefinition, implementation, declaration, hover, references, rename",
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
      id: "PBI-BRIDGE-RENAME",
      story: {
        role: "Lua developer editing markdown",
        capability: "rename symbols in Lua code blocks within markdown files",
        benefit: "I can refactor variable and function names consistently across all usages in injected code",
      },
      acceptance_criteria: [
        {
          criterion: "build_bridge_rename_request creates valid JSON-RPC request with position + newName",
          verification: "Unit test: request includes textDocument.uri, position (transformed to virtual coordinates), and newName parameter",
        },
        {
          criterion: "transform_rename_response_to_host handles WorkspaceEdit.changes format",
          verification: "Unit test: transforms URI keys from virtual to host and all TextEdit.range coordinates",
        },
        {
          criterion: "transform_rename_response_to_host handles WorkspaceEdit.documentChanges format",
          verification: "Unit test: transforms TextDocumentEdit.textDocument.uri and all TextEdit.range coordinates in edits array",
        },
        {
          criterion: "send_rename_request in text_document/rename.rs follows established module pattern",
          verification: "Code review: module structure matches definition.rs, references.rs patterns",
        },
        {
          criterion: "lsp_impl.rs rename handler integrates bridge for injection regions",
          verification: "Integration test: rename request to injection region returns transformed WorkspaceEdit",
        },
        {
          criterion: "E2E test verifies rename works in markdown Lua code blocks",
          verification: "E2E test: vim.lsp.buf.rename in Lua code block renames all occurrences with correct positions",
        },
      ],
      status: "ready",
      refinement_notes: [
        "WorkspaceEdit response format is more complex than Location[]: requires URI key transformation in changes object and URI field transformation in documentChanges array",
        "Response can contain either 'changes' (uri -> TextEdit[]) or 'documentChanges' (TextDocumentEdit[] | mixed resource operations)",
        "transform_rename_response_to_host is NEW - cannot reuse transform_definition_response_to_host",
        "Pattern: build_bridge_rename_request (protocol.rs) + transform_rename_response_to_host (protocol.rs) + send_rename_request (text_document/rename.rs)",
        "Request params include position (needs transformation) and newName string (pass through)",
        "Reference files: definition.rs for module pattern, protocol.rs for transform function patterns",
      ],
    },
  ],
  sprint: {
    number: 170,
    pbi_id: "PBI-BRIDGE-RENAME",
    goal: "Implement textDocument/rename bridging with WorkspaceEdit response transformation to enable symbol renaming in injected code blocks",
    status: "planning",
    subtasks: [
      {
        test: "Unit test: build_bridge_rename_request creates valid JSON-RPC request with position + newName params",
        implementation: "Add build_bridge_rename_request function to protocol.rs with virtual URI, transformed position, and newName parameter",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Pattern: same as build_bridge_references_request but with newName instead of context.includeDeclaration"],
      },
      {
        test: "Unit test: transform_rename_response_to_host handles WorkspaceEdit.changes format (uri -> TextEdit[] map)",
        implementation: "Add transform_rename_response_to_host function to protocol.rs that transforms URI keys and TextEdit.range coordinates in changes object",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["NEW transform function - cannot reuse transform_definition_response_to_host", "WorkspaceEdit.changes is a map with URI keys that need transformation", "Each TextEdit in the array needs range transformation"],
      },
      {
        test: "Unit test: transform_rename_response_to_host handles WorkspaceEdit.documentChanges format (TextDocumentEdit[])",
        implementation: "Extend transform_rename_response_to_host to handle documentChanges array with textDocument.uri and edits[].range transformation",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["documentChanges can contain TextDocumentEdit[] or mixed resource operations", "Focus on TextDocumentEdit: { textDocument: { uri }, edits: TextEdit[] }", "Each TextEdit.range needs coordinate transformation"],
      },
      {
        test: "Integration test: send_rename_request in text_document/rename.rs follows established module pattern",
        implementation: "Create rename.rs module with send_rename_request method on LanguageServerPool following references.rs pattern",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Follow references.rs structure: imports, didOpen handling, request/response cycle", "Add newName parameter to method signature"],
      },
      {
        test: "Integration test: lsp_impl.rs rename handler returns transformed WorkspaceEdit for injection regions",
        implementation: "Wire rename handler in lsp_impl.rs with renameProvider capability, detect injection region and delegate to bridge",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Add renameProvider: true to server capabilities", "Follow pattern from references handler: detect injection, get region info, call bridge"],
      },
      {
        test: "E2E test: vim.lsp.buf.rename in Lua code block renames all occurrences with correct positions",
        implementation: "Add Neovim E2E test verifying rename in markdown Lua code block applies edits at correct host positions",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Use retry_for_lsp_indexing helper for async indexing", "Verify renamed text appears at expected locations in host document"],
      },
    ],
  },
  completed: [
    { number: 169, pbi_id: "PBI-BRIDGE-REFERENCES", goal: "Implement textDocument/references bridging to enable finding all usages of symbols in injected code blocks", status: "done", subtasks: [] },
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
    { sprint: 169, improvements: [
      { action: "References completes LSP navigation feature set - pattern from Sprints 164-168 applies directly", timing: "immediate", status: "completed", outcome: "All 5 navigation methods (definition, typeDefinition, implementation, declaration, references) now support injected code blocks with coordinate and URI transformation. References uses same transform_definition_response_to_host since Location[] format is identical." },
      { action: "Context parameters (includeDeclaration) pass through seamlessly - no special handling needed beyond signature", timing: "immediate", status: "completed", outcome: "LSP methods with context parameters require only signature extension, not new transformation logic. Pattern: accept extra params in send_*_request(), pass to build_bridge_*_request(), include in JSON payload. Future methods with context (documentSymbol, etc.) follow same approach." },
    ]},
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
