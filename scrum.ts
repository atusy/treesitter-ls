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
    // Completed: PBI-144 (Sprint 114) - async bridge foundation fixes
    {
      id: "PBI-141",
      story: {
        role: "developer editing Lua files",
        capability: "have go-to-definition requests in Markdown code blocks use fully async I/O",
        benefit: "definition responses are faster and don't block other LSP requests while waiting for lua-language-server",
      },
      acceptance_criteria: [
        {
          criterion: "TokioAsyncLanguageServerPool.goto_definition() method implemented with async request/response pattern",
          verification: "Unit test verifies goto_definition returns valid Location response",
        },
        {
          criterion: "definition_impl uses async pool.goto_definition() instead of spawn_blocking",
          verification: "grep confirms no spawn_blocking in definition.rs for bridged requests",
        },
        {
          criterion: "Go-to-definition requests to lua-language-server return valid responses through async path",
          verification: "E2E test opens Markdown with Lua code block, requests definition, receives location",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-142",
      story: {
        role: "Rustacean editing Markdown",
        capability: "have completion requests in Markdown code blocks use fully async I/O",
        benefit: "completion responses are faster and don't block other LSP requests while waiting for rust-analyzer",
      },
      acceptance_criteria: [
        {
          criterion: "TokioAsyncLanguageServerPool.completion() method implemented with async request/response pattern",
          verification: "Unit test verifies completion returns valid CompletionList response",
        },
        {
          criterion: "completion handler uses async pool.completion() for bridged requests",
          verification: "grep confirms async completion path in lsp_impl.rs",
        },
        {
          criterion: "Completion requests to rust-analyzer return valid responses through async path",
          verification: "E2E test opens Markdown with Rust code block, requests completion, receives items",
        },
      ],
      status: "ready",
    },
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

  sprint: null, // Sprint 114 completed - see completed array

  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },

  // Historical sprints (recent 2) | Sprint 1-113: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 114, pbi_id: "PBI-144", goal: "Fix async bridge foundation by adding cwd parameter to spawn, converting sync I/O to async (tokio::fs), and removing dead_code annotations now that the async bridge is wired into production", status: "done", subtasks: [] },
    { number: 113, pbi_id: "PBI-140", goal: "Implement fully async hover bridging with TokioAsyncBridgeConnection reader task, TokioAsyncLanguageServerPool, and wire into hover_impl to replace spawn_blocking pattern", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-112: modular refactoring pattern, E2E indexing waits
  retrospectives: [
    {
      sprint: 114,
      improvements: [
        { action: "External code reviews (Gemini Code Assist, Copilot) identify real issues before merge - integrate AI review tools as part of Definition of Done for complex PRs", timing: "sprint", status: "active", outcome: null },
        { action: "Removing #[allow(dead_code)] reveals unused fields and forces proper resource management - Drop implementation added for TokioAsyncBridgeConnection to use reader_handle/shutdown_tx", timing: "immediate", status: "completed", outcome: "Proper Drop impl joins reader task and sends shutdown signal; no resource leaks" },
        { action: "User story benefit should be user-centric not technical - refined mid-sprint from 'async pool sends proper shutdown' to 'diagnostics, hover, and other LSP features stay responsive during language server initialization'", timing: "immediate", status: "completed", outcome: "PBI-144 benefit now describes user value, not implementation detail" },
        { action: "Two flaky tests related to rust-analyzer contention still exist - investigate and fix: tests may need better isolation or longer timeouts for CI environment", timing: "sprint", status: "active", outcome: null },
        { action: "Copilot review Issue #4 (missing error logging when hover=None) - add tracing::debug! or tracing::warn! when bridged hover returns None for observability", timing: "sprint", status: "active", outcome: null },
      ],
    },
    {
      sprint: 113,
      improvements: [
        { action: "INVEST-compliant vertical slice pattern validated - PBI-140 delivered user value (faster hover responses) by combining infrastructure (TokioAsyncBridgeConnection), wiring (TokioAsyncLanguageServerPool into TreeSitterLs), and E2E test in single PBI", timing: "immediate", status: "completed", outcome: "Avoided PBI-091 anti-pattern where infrastructure was never wired; async pool is now used in production hover path" },
        { action: "Apply vertical slice pattern to PBI-142 (completion + signatureHelp) - each PBI should deliver observable user value, not just infrastructure changes", timing: "sprint", status: "active", outcome: null },
        { action: "Sync bridge module has potential flaky test (read_response_for_id_with_notifications_returns_none_on_timeout uses 100ms timeout with 5s assertion slack) - monitor for CI failures, consider increasing timeout margin if flaky", timing: "sprint", status: "active", outcome: null },
        { action: "tokio::select! pattern enables clean async shutdown - reader task completes within 100ms when idle (AC1 verified), unlike sync read_line which blocks until data arrives", timing: "immediate", status: "completed", outcome: "Test shutdown_while_reader_idle_completes_within_100ms passes consistently" },
        { action: "Remove #[allow(dead_code)] from TokioAsyncBridgeConnection and TokioAsyncLanguageServerPool now that they are wired into production code (hover path uses them)", timing: "sprint", status: "completed", outcome: "Tracked as AC4 in PBI-144" },
      ],
    },
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
