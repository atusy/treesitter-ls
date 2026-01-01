// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Rustacean editing Markdown",
  "developer editing Lua files",
  "documentation author with Rust code blocks",
] as const satisfies readonly string[]; // Must have at least one role. Avoid generic roles like "user" or "admin". Remove obsolete roles freely.

const scrum: ScrumDashboard = {
  product_goal: {
    statement:
      "Expand LSP bridge to support most language server features indirectly through bridging (ADR-0006, 0007, 0008)",
    success_metrics: [
      {
        metric: "Bridge coverage",
        target:
          "Support completion, signatureHelp, references, rename, codeAction, formatting, typeDefinition, implementation, documentHighlight, declaration, inlayHint, callHierarchy, typeHierarchy, documentLink, foldingRange",
      },
      {
        metric: "Modular architecture",
        target: "Bridge module organized with text_document/ subdirectory matching lsp_impl structure",
      },
      {
        metric: "E2E test coverage",
        target: "Each bridged feature has E2E test verifying end-to-end flow",
      },
    ],
  },

  // Completed PBIs: PBI-001 through PBI-140 (Sprint 1-113) | History: git log -- scrum.yaml, scrum.ts
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  product_backlog: [
    // ADR-0009 Implementation: Vertical slices with user-facing value
    // Completed: PBI-144 (Sprint 114), PBI-145 (Sprint 115), PBI-148 (Sprint 116), PBI-146 (Sprint 117), PBI-149 (Sprint 118), PBI-141 (Sprint 119), PBI-142 (Sprint 120)
    // Rejected: PBI-147 (wait for indexing) - replaced by PBI-149
    {
      id: "PBI-143",
      story: {
        role: "Rustacean editing Markdown",
        capability: "have signatureHelp requests in Markdown code blocks use fully async I/O",
        benefit: "signature help responses are faster and show parameter hints without blocking",
      },
      acceptance_criteria: [
        {
          criterion: "TokioAsyncLanguageServerPool.signature_help() method implemented with async request/response pattern",
          verification: "Unit test verifies signature_help returns valid SignatureHelp response",
        },
        {
          criterion: "signatureHelp handler uses async pool.signature_help() for bridged requests",
          verification: "grep confirms async signature_help path in lsp_impl.rs",
        },
        {
          criterion: "SignatureHelp requests to rust-analyzer return valid responses through async path",
          verification: "E2E test opens Markdown with Rust code block, requests signatureHelp, receives signatures",
        },
      ],
      status: "ready",
    },
  ],

  sprint: {
    number: 121,
    pbi_id: "PBI-143",
    goal: "Implement fully async signatureHelp for Rust code blocks in Markdown, completing ADR-0009 async migration for high-frequency LSP methods",
    status: "in_progress",
    subtasks: [
      {
        test: "Unit test verifies TokioAsyncLanguageServerPool.signature_help() returns Option<SignatureHelp>",
        implementation: "Add signature_help() method to TokioAsyncLanguageServerPool following hover/completion pattern with ServerState tracking",
        type: "behavioral",
        status: "green",
        commits: [],
        notes: ["Follow existing hover()/completion() pattern", "Reuse sync_document for didOpen/didChange", "30s timeout matching other methods"],
      },
      {
        test: "grep confirms signatureHelp handler uses async pool.signature_help() path instead of spawn_blocking",
        implementation: "Modify signature_help.rs to use tokio_async_pool.signature_help() for bridged requests",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Replace spawn_blocking with async pool call", "Remove sync connection take/return pattern", "Forward notifications via pool channel"],
      },
      {
        test: "E2E test test_lsp_signature_help.lua passes with async implementation",
        implementation: "Verify existing E2E test works with new async path - no changes expected if implementation correct",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Existing test should pass unchanged", "May need timeout adjustment per Sprint 120 lesson (15s -> 90s pattern)"],
      },
    ],
  },

  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },

  // Historical sprints (recent 2) | Sprint 1-119: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 120, pbi_id: "PBI-142", goal: "Implement fully async completion with TokioAsyncLanguageServerPool", status: "done", subtasks: [] },
    { number: 119, pbi_id: "PBI-141", goal: "Implement fully async goto_definition with ServerState tracking", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-116: modular refactoring pattern, E2E indexing waits, vertical slice validation, RAII cleanup
  retrospectives: [
    { sprint: 120, improvements: [
      { action: "Pattern established: hover/goto_definition/completion share identical structure - consider extracting common async request handler", timing: "sprint", status: "active", outcome: null },
      { action: "E2E timeout pattern emerging (15s -> 90s for async indexing) - document timeout rationale in test files", timing: "immediate", status: "active", outcome: null },
      { action: "Sprint 117 lesson (document version tracking) successfully prevented regression - continue applying lessons from previous retrospectives", timing: "sprint", status: "active", outcome: null },
      { action: "Three async methods (hover, goto_definition, completion) follow same pattern - opportunity for DRY refactoring with generic request handler", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 118, improvements: [
      { action: "Extract common wait_for_indexing loop logic - avoid duplication between with_timeout and with_forward variants", timing: "immediate", status: "completed", outcome: "wait_for_indexing_impl(receiver, timeout, forward_to: Option<&Sender>) consolidates loop logic" },
      { action: "Document background task lifetimes in async initialization - clarify forwarder task lifecycle in spawn_and_initialize docstring", timing: "immediate", status: "completed", outcome: "Added 'Notification Forwarding Lifecycle' section documenting channel closure conditions" },
      { action: "Pattern: Local channel + forwarder for async initialization with notification filtering", timing: "sprint", status: "active", outcome: null },
      { action: "Consider indexing timeout configurability PBI - allow users to trade accuracy vs responsiveness", timing: "product", status: "active", outcome: null },
    ] },
  ],
};

// ============================================================
// Type Definitions (DO NOT MODIFY - request human review for schema changes)
// ============================================================

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
