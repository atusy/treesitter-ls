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
    number: 163,
    pbi_id: "PBI-REFACTOR-DIDCHANGE-MODULE",
    goal: "Extract didChange logic to text_document/did_change.rs module for consistent architecture",
    status: "planning",
    subtasks: [
      {
        test: "Verify did_change.rs module exists and exports forward_didchange_to_opened_docs function",
        implementation: "Create src/lsp/bridge/text_document/did_change.rs with forward_didchange_to_opened_docs moved from pool.rs",
        type: "structural",
        status: "pending",
        commits: [],
        notes: ["Extract as impl LanguageServerPool block following did_close.rs pattern"],
      },
      {
        test: "Verify send_didchange_for_virtual_doc helper exists in did_change.rs module",
        implementation: "Move send_didchange_for_virtual_doc from pool.rs to did_change.rs as impl method",
        type: "structural",
        status: "pending",
        commits: [],
        notes: ["Private helper method for sending individual didChange notifications"],
      },
      {
        test: "Verify pool.rs provides get_host_virtual_docs accessor for did_change module",
        implementation: "Add get_host_virtual_docs() helper method to pool.rs with pub(super) visibility",
        type: "structural",
        status: "pending",
        commits: [],
        notes: ["Accessor returns cloned Vec<OpenedVirtualDoc> without removing - unlike remove_host_virtual_docs in did_close"],
      },
      {
        test: "Verify text_document.rs includes did_change module",
        implementation: "Add 'mod did_change;' to text_document.rs",
        type: "structural",
        status: "pending",
        commits: [],
        notes: ["Module organization matches did_close, hover, completion, signature_help pattern"],
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
        notes: ["E2E tests including e2e_didchange_forwarding must pass"],
      },
    ],
  },
  completed: [
    { number: 162, pbi_id: "PBI-REFACTOR-DIDCLOSE-MODULE", goal: "Extract didClose logic to text_document/did_close.rs module for consistent architecture", status: "done", subtasks: [] },
    { number: 161, pbi_id: "PBI-DIDCHANGE-FORWARDING", goal: "Forward didChange notifications from host documents to opened virtual documents", status: "done", subtasks: [] },
    { number: 160, pbi_id: "PBI-DIDCLOSE-FORWARDING", goal: "Propagate didClose from host documents to virtual documents ensuring proper cleanup without closing connections", status: "done", subtasks: [] },
  ],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
  retrospectives: [
    { sprint: 162, improvements: [
      { action: "Use pub(super) accessor pattern for module extraction", timing: "immediate", status: "completed", outcome: "connections(), remove_host_virtual_docs(), remove_document_version() accessors enable did_close.rs to access pool.rs private fields while maintaining encapsulation - reusable pattern for upcoming did_change.rs extraction" },
    ]},
    { sprint: 161, improvements: [
      { action: "Build dependency chains incrementally for cohesive delivery", timing: "immediate", status: "completed", outcome: "Sprints 159→160→161 chain (region_id → didClose → didChange) enabled incremental delivery" },
    ]},
    { sprint: 160, improvements: [
      { action: "Separate document lifecycle from connection lifecycle", timing: "immediate", status: "completed", outcome: "didClose removes virtual doc but keeps connection open - enables efficient server reuse" },
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
