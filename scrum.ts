// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Lua developer editing markdown",
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
      id: "PBI-SIGNATURE-HELP-BRIDGE",
      story: {
        role: "Lua developer editing markdown",
        capability: "see function parameter hints when typing function calls in Lua code blocks",
        benefit: "I can quickly understand what arguments a function expects without leaving my markdown document",
      },
      acceptance_criteria: [
        {
          criterion: "Typing '(' after a Lua function name in a markdown code block triggers signature help from lua-language-server",
          verification: "cargo test --test e2e_lsp_lua_signature_help --features e2e",
        },
        {
          criterion: "Typing ',' between function arguments re-triggers signature help highlighting the next parameter",
          verification: "Manual test: open markdown with ```lua block, type 'string.format(' and verify signature appears, type ',' and verify active parameter advances",
        },
        {
          criterion: "Position coordinates are correctly transformed between host markdown and virtual Lua document",
          verification: "cargo test bridge_signature_help --lib -- tests in src/lsp/bridge/protocol.rs",
        },
        {
          criterion: "SignatureHelp response includes activeParameter and activeSignature fields when provided by downstream server",
          verification: "cargo test transform_signature_help --lib",
        },
      ],
      status: "ready",
      refinement_notes: [
        "Bridge architecture pattern established: handler -> pool -> protocol (see hover.rs, completion.rs)",
        "Capability already advertised in lsp_impl.rs with trigger_characters ['(', ',']",
        "Stub exists at src/lsp/lsp_impl/text_document/signature_help.rs (returns Ok(None))",
        "Implementation files needed: bridge/text_document/signature_help.rs, protocol additions",
        "E2E test pattern: see tests/e2e_lsp_lua_hover.rs for reference",
        "SignatureHelp response may contain ranges that need coordinate transformation",
      ],
    },
  ],
  sprint: {
    number: 158,
    pbi_id: "PBI-SIGNATURE-HELP-BRIDGE",
    goal: "Enable signature help bridging for Lua code blocks in markdown documents",
    status: "planning",
    subtasks: [
      {
        test: "build_bridge_signature_help_request translates position to virtual coordinates",
        implementation: "Add build_bridge_signature_help_request() function to protocol.rs",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Follow hover request pattern: virtual URI + position translation"],
      },
      {
        test: "transform_signature_help_response_to_host passes through null result unchanged",
        implementation: "Add transform_signature_help_response_to_host() handling null case",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["SignatureHelp may be null when no signature available"],
      },
      {
        test: "transform_signature_help_response_to_host preserves activeParameter and activeSignature",
        implementation: "Extend transform function to preserve signature help metadata",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["These fields indicate current parameter position - must not be modified"],
      },
      {
        test: "send_signature_help_request returns signature help from downstream server",
        implementation: "Add send_signature_help_request() to bridge/text_document/signature_help.rs",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Follow hover.rs pattern: get connection, send didOpen if needed, send request, transform response"],
      },
      {
        test: "Wire signature_help module into bridge/text_document.rs",
        implementation: "Add mod signature_help to bridge/text_document.rs",
        type: "structural",
        status: "pending",
        commits: [],
        notes: ["Module wiring only, no new behavior"],
      },
      {
        test: "signature_help_impl delegates to bridge for injection regions",
        implementation: "Update signature_help_impl() in lsp_impl/text_document/signature_help.rs",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Follow hover_impl pattern: detect injection, get server config, call pool method"],
      },
      {
        test: "E2E: typing '(' after Lua function triggers signature help",
        implementation: "Create e2e_lsp_lua_signature_help.rs following hover E2E pattern",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Test with string.format( to verify end-to-end flow"],
      },
    ],
  },
  completed: [
    { number: 157, pbi_id: "PBI-REQUEST-ID-SERVICE-WRAPPER", goal: "Pass upstream request IDs to downstream servers via tower Service wrapper per ADR-0016", status: "done", subtasks: [] },
    { number: 156, pbi_id: "PBI-REQUEST-ID-PASSTHROUGH", goal: "Validate ADR-0016 request ID semantics (research sprint)", status: "done", subtasks: [] },
    { number: 155, pbi_id: "PBI-RETRY-FAILED-CONNECTION", goal: "Enable automatic retry when downstream server connection has failed", status: "done", subtasks: [] },
    { number: 154, pbi_id: "PBI-STATE-PER-CONNECTION", goal: "Move ConnectionState to per-connection ownership fixing race condition", status: "done", subtasks: [] },
    { number: 153, pbi_id: "PBI-WIRE-FAILED-STATE", goal: "Return REQUEST_FAILED when downstream server has failed initialization", status: "done", subtasks: [] },
    { number: 152, pbi_id: "PBI-REQUEST-FAILED-INIT", goal: "Return REQUEST_FAILED immediately during initialization instead of blocking", status: "done", subtasks: [] },
    { number: 151, pbi_id: "PBI-INIT-TIMEOUT", goal: "Add timeout to initialization to prevent infinite hang", status: "done", subtasks: [] },
  ],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
  retrospectives: [
    { sprint: 157, improvements: [
      { action: "Tower Service middleware pattern for cross-cutting concerns", timing: "immediate", status: "completed", outcome: "RequestIdCapture wrapper injects behavior without modifying core handler logic" },
      { action: "Task-local storage for request-scoped context", timing: "immediate", status: "completed", outcome: "tokio::task_local! provides clean request-scoped state without parameter threading" },
      { action: "Validate framework capabilities before concluding 'impossible'", timing: "immediate", status: "completed", outcome: "Sprint 156 prematurely concluded tower-lsp limitations; Service layer approach discovered via user feedback" },
      { action: "Fixed ID for internal requests", timing: "immediate", status: "completed", outcome: "Initialize handshake uses ID=0 for bridge-originated requests vs upstream-originated" },
    ]},
    { sprint: 156, improvements: [
      { action: "Investigate framework constraints before planning", timing: "immediate", status: "completed", outcome: "tower-lsp LanguageServer trait doesn't expose IDs, but Service wrapper can" },
      { action: "Distinguish ADR intent vs literal interpretation", timing: "immediate", status: "completed", outcome: "ADR-0016 intent achievable via Service wrapper pattern" },
      { action: "Research sprints are valid outcomes", timing: "immediate", status: "completed", outcome: "Research led to Service wrapper discovery - PBI-REQUEST-ID-SERVICE-WRAPPER created" },
    ]},
    { sprint: 155, improvements: [
      { action: "Box::pin for recursive async calls", timing: "immediate", status: "completed", outcome: "Recursive retry compiles" },
    ]},
    { sprint: 154, improvements: [
      { action: "Per-connection state via ConnectionHandle", timing: "immediate", status: "completed", outcome: "Race conditions fixed" },
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
