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
      id: "PBI-REFACTOR-DIDCLOSE-MODULE",
      story: {
        role: "Lua developer editing markdown",
        capability: "I want didClose logic organized in src/lsp/bridge/text_document/did_close.rs",
        benefit: "So that the codebase follows consistent modular architecture matching lsp_impl structure",
      },
      acceptance_criteria: [
        { criterion: "didClose logic extracted to text_document/did_close.rs", verification: "File exists with send_didclose_notification and close_host_document logic" },
        { criterion: "pool.rs imports and delegates to did_close module", verification: "pool.rs uses pub(super) functions from did_close.rs" },
        { criterion: "All existing tests pass unchanged", verification: "make test && make test_e2e pass" },
      ],
      status: "ready",
    },
    {
      id: "PBI-REFACTOR-DIDCHANGE-MODULE",
      story: {
        role: "Lua developer editing markdown",
        capability: "I want didChange logic organized in src/lsp/bridge/text_document/did_change.rs",
        benefit: "So that the codebase follows consistent modular architecture matching lsp_impl structure",
      },
      acceptance_criteria: [
        { criterion: "didChange logic extracted to text_document/did_change.rs", verification: "File exists with forward_didchange_to_opened_docs and send_didchange_for_virtual_doc logic" },
        { criterion: "pool.rs imports and delegates to did_change module", verification: "pool.rs uses pub(super) functions from did_change.rs" },
        { criterion: "All existing tests pass unchanged", verification: "make test && make test_e2e pass" },
      ],
      status: "ready",
    },
  ],
  sprint: {
    number: 162,
    pbi_id: "PBI-REFACTOR-DIDCLOSE-MODULE",
    goal: "Extract didClose logic to text_document/did_close.rs module for consistent architecture",
    status: "planning",
    subtasks: [
      {
        test: "Verify did_close.rs module exists and exports send_didclose_notification function",
        implementation: "Create src/lsp/bridge/text_document/did_close.rs with send_didclose_notification moved from pool.rs",
        type: "structural",
        status: "pending",
        commits: [],
        notes: ["Extract send_didclose_notification as standalone function taking &LanguageServerPool"],
      },
      {
        test: "Verify close_host_document function exists in did_close.rs module",
        implementation: "Move close_host_document from pool.rs to did_close.rs as standalone function",
        type: "structural",
        status: "pending",
        commits: [],
        notes: ["close_host_document delegates to send_didclose_notification internally"],
      },
      {
        test: "Verify text_document/mod.rs includes did_close module",
        implementation: "Add 'mod did_close;' to text_document.rs",
        type: "structural",
        status: "pending",
        commits: [],
        notes: ["Module organization matches existing hover, completion, signature_help pattern"],
      },
      {
        test: "Verify pool.rs delegates to did_close module functions",
        implementation: "Update pool.rs to import and delegate send_didclose_notification and close_host_document to did_close module",
        type: "structural",
        status: "pending",
        commits: [],
        notes: ["Keep public API unchanged - wrapper methods delegate to did_close functions"],
      },
      {
        test: "All existing unit tests pass (make test)",
        implementation: "Run make test to verify no behavioral changes",
        type: "structural",
        status: "pending",
        commits: [],
        notes: ["Pure structural refactoring - all tests must pass unchanged"],
      },
      {
        test: "All E2E tests pass (make test_e2e)",
        implementation: "Run make test_e2e to verify end-to-end functionality preserved",
        type: "structural",
        status: "pending",
        commits: [],
        notes: ["E2E tests verify didClose forwarding works correctly after refactoring"],
      },
    ],
  },
  completed: [
    { number: 161, pbi_id: "PBI-DIDCHANGE-FORWARDING", goal: "Forward didChange notifications from host documents to opened virtual documents", status: "done", subtasks: [] },
    { number: 160, pbi_id: "PBI-DIDCLOSE-FORWARDING", goal: "Propagate didClose from host documents to virtual documents ensuring proper cleanup without closing connections", status: "done", subtasks: [] },
    { number: 159, pbi_id: "PBI-STABLE-REGION-ID", goal: "Implement stable region_id for shared virtual document URIs across bridge features", status: "done", subtasks: [] },
    { number: 158, pbi_id: "PBI-SIGNATURE-HELP-BRIDGE", goal: "Enable signature help bridging for Lua code blocks in markdown documents", status: "done", subtasks: [] },
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
    { sprint: 161, improvements: [
      { action: "Build dependency chains incrementally for cohesive delivery", timing: "immediate", status: "completed", outcome: "Sprints 159→160→161 chain (region_id → didClose → didChange) enabled incremental delivery - each sprint addressed specific concern while building stable foundation for next feature" },
      { action: "Use skip-if-not-opened pattern for LSP protocol compliance", timing: "immediate", status: "completed", outcome: "forward_didchange_to_opened_docs checks host_to_virtual before sending - prevents protocol violations (didChange without didOpen), ensures downstream servers receive valid notification sequences" },
      { action: "Document deferred optimizations with TODO comments", timing: "immediate", status: "completed", outcome: "TODO comment for incremental sync documents trade-off decision - full sync is simpler/pragmatic choice now, TODO preserves optimization opportunity for future without blocking current delivery" },
      { action: "Leverage stable foundations to accelerate implementation", timing: "immediate", status: "completed", outcome: "Building on Sprint 160's host_to_virtual map made didChange straightforward - stable infrastructure enables rapid feature addition with minimal complexity" },
    ]},
    { sprint: 160, improvements: [
      { action: "Design data structures in conversation before implementation", timing: "immediate", status: "completed", outcome: "Upfront discussion of OpenedVirtualDoc structure clarified requirements - implementation became straightforward with clear field ownership (virtual_uri, host_uri, region_id)" },
      { action: "Store computed values rather than recomputing", timing: "immediate", status: "completed", outcome: "virtual_uri stored directly in OpenedVirtualDoc instead of reconstructing from host_uri + region_id - simpler code, safer access pattern, single source of truth" },
      { action: "Best-effort cleanup pattern for non-critical operations", timing: "immediate", status: "completed", outcome: "didClose errors don't block cleanup of other virtual docs - continue_on_error pattern prevents cascading failures during document lifecycle management" },
      { action: "Separate document lifecycle from connection lifecycle", timing: "immediate", status: "completed", outcome: "didClose removes virtual doc but keeps connection open - enables efficient server reuse across multiple code blocks, established clear responsibility boundaries" },
    ]},
    { sprint: 159, improvements: [
      { action: "User conversation as refinement tool", timing: "immediate", status: "completed", outcome: "Discussing didChange forwarding implementation revealed hidden technical debt in region_id calculation - conversation-driven discovery led to clear User Story and acceptance criteria" },
      { action: "Fix foundational issues before building on them", timing: "immediate", status: "completed", outcome: "Stable region_id is prerequisite for didClose forwarding - addressing technical debt enables future features rather than accumulating workarounds" },
      { action: "Check all similar code when fixing patterns", timing: "immediate", status: "completed", outcome: "Found signature_help.rs had same 'temp' hardcoded region_id issue as hover.rs and completion.rs - comprehensive fix benefited all three bridge features" },
      { action: "Per-language ordinal counting provides stable identifiers", timing: "immediate", status: "completed", outcome: "Format {language}-{ordinal} ensures inserting Python blocks between Lua blocks preserves lua-0, lua-1 ordinals - simple approach without complex heuristics" },
    ]},
    { sprint: 158, improvements: [
      { action: "Well-established patterns accelerate implementation", timing: "immediate", status: "completed", outcome: "Following hover.rs and completion.rs patterns made signature_help.rs straightforward - consistent structure across text_document/ features" },
      { action: "Simpler features validate pattern robustness", timing: "immediate", status: "completed", outcome: "SignatureHelp required no range transformation (unlike completion), proving pattern handles varying complexity levels" },
      { action: "Pattern template for remaining bridge features", timing: "immediate", status: "completed", outcome: "Established pattern: pool method + protocol helpers + lsp_impl integration + E2E test - ready for codeAction and definition" },
      { action: "TDD catches integration issues early", timing: "immediate", status: "completed", outcome: "E2E tests verified full bridge wiring including request ID passthrough from Sprint 157" },
    ]},
    { sprint: 157, improvements: [{ action: "Tower Service middleware for cross-cutting concerns", timing: "immediate", status: "completed", outcome: "RequestIdCapture wrapper + task-local storage" }] },
    { sprint: 156, improvements: [{ action: "Research sprints are valid outcomes", timing: "immediate", status: "completed", outcome: "Research led to Service wrapper discovery" }] },
    { sprint: 155, improvements: [{ action: "Box::pin for recursive async calls", timing: "immediate", status: "completed", outcome: "Recursive retry compiles" }] },
    { sprint: 154, improvements: [{ action: "Per-connection state via ConnectionHandle", timing: "immediate", status: "completed", outcome: "Race conditions fixed" }] },
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
