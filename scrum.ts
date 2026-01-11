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
      id: "PBI-DIDCHANGE-FORWARDING",
      story: {
        role: "Lua developer editing markdown",
        capability: "I want to propagate change of the host document to the virtual documents attached to bridged downstream language servers",
        benefit: "So that I can use features with fresh content",
      },
      acceptance_criteria: [
        {
          criterion: "didChange sent only for opened virtual documents",
          verification: "Skip notification if virtual doc not in host_to_virtual map",
        },
        {
          criterion: "Full content sync used (not incremental)",
          verification: "Use build_bridge_didchange_notification() with TextDocumentSyncKind::Full",
        },
        {
          criterion: "Features work with fresh content after host change",
          verification: "E2E test: edit Lua block, verify completion reflects changes",
        },
        {
          criterion: "TODO comment for incremental sync optimization",
          verification: "Code contains TODO comment noting future incremental didChange support",
        },
      ],
      status: "ready",
    },
  ],
  sprint: {
    number: 161,
    pbi_id: "PBI-DIDCHANGE-FORWARDING",
    goal: "Forward didChange notifications from host documents to opened virtual documents ensuring downstream language servers receive fresh content",
    status: "in_progress",
    subtasks: [
      {
        test: "Test forward_didchange_to_opened_docs sends didChange only for opened virtual documents",
        implementation: "Add forward_didchange_to_opened_docs method to LanguageServerPool that checks host_to_virtual and sends didChange only for opened docs",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "cb8d5776", message: "feat(bridge): forward didChange notifications to opened virtual documents", phase: "green" }],
        notes: ["Use build_bridge_didchange_notification with TextDocumentSyncKind::Full", "Skip notification if virtual doc not in host_to_virtual map"],
      },
      {
        test: "Test forward_didchange_to_opened_docs skips unopened virtual documents",
        implementation: "Verify that virtual documents not yet opened (no didOpen sent) are not sent didChange notifications",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "cb8d5776", message: "feat(bridge): forward didChange notifications to opened virtual documents", phase: "green" }],
        notes: ["Important: only opened docs should receive didChange", "Prevents sending updates to unknown documents"],
      },
      {
        test: "Test lsp_impl::did_change collects injection regions and forwards changes",
        implementation: "Wire forward_didchange_to_opened_docs in lsp_impl.rs after parse_document, collecting injection regions with content",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "cb8d5776", message: "feat(bridge): forward didChange notifications to opened virtual documents", phase: "green" }],
        notes: ["Data flow: parse_document -> collect injections -> forward_didchange_to_opened_docs", "Each injection has: language, region_id, content"],
      },
      {
        test: "E2E test: edit Lua block in markdown and verify completion reflects changes",
        implementation: "Add E2E test that modifies Lua code block content and verifies subsequent LSP features use fresh content",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Validates end-to-end flow: host edit -> virtual doc update -> downstream LS response", "Use retry_for_lsp_indexing pattern for async operations"],
      },
      {
        test: "Verify TODO comment exists for incremental sync optimization",
        implementation: "Add TODO comment noting future opportunity to support incremental didChange instead of full sync",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Full sync is simpler but less efficient", "Incremental sync requires tracking content deltas per injection region"],
      },
    ],
  },
  completed: [
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
