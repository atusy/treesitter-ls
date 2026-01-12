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
      id: "PBI-BRIDGE-DEFINITION",
      story: {
        role: "Lua developer editing markdown",
        capability: "navigate to Lua function definitions from within markdown code blocks using goto definition",
        benefit: "I can explore Lua code structure without leaving my markdown documentation",
      },
      acceptance_criteria: [
        {
          criterion: "textDocument/definition request in Lua code block returns definition location from lua-language-server",
          verification: "E2E test: open markdown with Lua code block, send definition request on function call, verify Location response with transformed line numbers",
        },
        {
          criterion: "Definition response ranges are transformed from virtual to host document coordinates",
          verification: "Unit test: verify transform_definition_response_to_host adds region_start_line to Location/LocationLink ranges",
        },
        {
          criterion: "Definition request position is transformed from host to virtual document coordinates",
          verification: "Unit test: verify build_bridge_definition_request subtracts region_start_line from position",
        },
        {
          criterion: "Bridge module follows text_document/<feature>.rs pattern",
          verification: "Code review: src/lsp/bridge/text_document/definition.rs exists with send_definition_request method on LanguageServerPool",
        },
        {
          criterion: "Handles both Location and LocationLink response formats per LSP spec",
          verification: "Unit tests: transform_definition_response_to_host handles Location[], LocationLink[], and null responses",
        },
      ],
      status: "done",
      refinement_notes: [
        "Pattern reference: hover.rs, completion.rs in src/lsp/bridge/text_document/",
        "Protocol helpers needed: build_bridge_definition_request, transform_definition_response_to_host in protocol.rs",
        "Definition responses may contain: Location (uri + range) or LocationLink (originSelectionRange + targetUri + targetRange + targetSelectionRange)",
        "Only targetRange and targetSelectionRange need transformation in LocationLink; originSelectionRange is already in host coordinates",
        "ADR alignment: Phase 1 of ADR-0013 scope, uses ADR-0014 async connection, ADR-0015 request ID passthrough, ADR-0016 pool coordination",
      ],
    },
  ],
  sprint: null,
  completed: [
    {
      number: 164,
      pbi_id: "PBI-BRIDGE-DEFINITION",
      goal: "Implement textDocument/definition bridging with coordinate transformation for Location and LocationLink response formats",
      status: "done",
      subtasks: [
      {
        test: "Unit test: build_bridge_definition_request subtracts region_start_line from position and uses virtual URI",
        implementation: "Add build_bridge_definition_request to protocol.rs following hover/completion pattern",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Pattern: build_bridge_hover_request in protocol.rs", "Position translation: host_line - region_start_line"],
      },
      {
        test: "Unit test: transform_definition_response_to_host handles Location[] with range transformation",
        implementation: "Add transform_definition_response_to_host to protocol.rs for Location array format",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Location has uri + range; only range.start.line and range.end.line need +region_start_line"],
      },
      {
        test: "Unit test: transform_definition_response_to_host handles LocationLink[] with targetRange and targetSelectionRange transformation",
        implementation: "Extend transform_definition_response_to_host for LocationLink format",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["LocationLink: originSelectionRange stays unchanged (host coords), targetRange and targetSelectionRange need transformation"],
      },
      {
        test: "Unit test: transform_definition_response_to_host handles null result",
        implementation: "Add null handling branch to transform_definition_response_to_host",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Follow pattern from transform_completion_response_to_host for null handling"],
      },
      {
        test: "Verify definition.rs module compiles with send_definition_request signature",
        implementation: "Create src/lsp/bridge/text_document/definition.rs with send_definition_request on LanguageServerPool",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Follow hover.rs pattern: get_or_create_connection, should_send_didopen, write_message loop"],
      },
      {
        test: "Verify bridge.rs and lsp_impl.rs compile with definition wiring",
        implementation: "Wire send_definition_request through bridge.rs to lsp_impl.rs goto_definition handler",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Check existing hover/completion wiring in bridge.rs and lsp_impl.rs for pattern"],
      },
      {
        test: "E2E test: definition in Lua code block returns transformed Location",
        implementation: "Add E2E test verifying end-to-end definition flow with lua-language-server",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Similar to E2E tests for hover/completion; verify Location line numbers are in host coordinates"],
      },
    ]},
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
