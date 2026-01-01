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
    // Completed: PBI-144 (Sprint 114), PBI-145 (Sprint 115) - async bridge + progress notifications
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

  sprint: null,

  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },

  // Historical sprints (recent 2) | Sprint 1-114: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 115, pbi_id: "PBI-145", goal: "Restore progress indicator visibility during language server indexing by wiring $/progress notification forwarding through the async bridge", status: "done", subtasks: [] },
    { number: 114, pbi_id: "PBI-144", goal: "Fix async bridge foundation by adding cwd parameter to spawn, converting sync I/O to async (tokio::fs), and removing dead_code annotations now that the async bridge is wired into production", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-113: modular refactoring pattern, E2E indexing waits, vertical slice validation
  retrospectives: [
    {
      sprint: 115,
      improvements: [
        { action: "External code reviews (Gemini Code Assist) catch real regressions - continue using AI review tools for complex PRs; PBI-145 was discovered this way", timing: "sprint", status: "completed", outcome: "$/progress notification regression identified and fixed within one sprint" },
        { action: "User challenge on priority was correct - when regression affects UX (progress indicators missing), prioritize immediately rather than deferring to future sprint", timing: "immediate", status: "completed", outcome: "PBI-145 prioritized as HIGH PRIORITY and completed same sprint it was identified" },
        { action: "Option<Receiver> pattern for one-time ownership transfer (take()) is clean and testable - apply this pattern when resources need single-use consumption", timing: "immediate", status: "completed", outcome: "notification_rx.take() in initialized() ensures forwarder starts exactly once" },
        { action: "Flaky tests need dedicated investigation time - did_open_uses_connection_info_write_virtual_file in sync bridge and E2E tests with rust-analyzer contention remain problematic", timing: "sprint", status: "active", outcome: null },
        { action: "AC3 (E2E verification of progress notifications) was not explicitly tested - relied on integration completeness; consider adding explicit E2E test for progress notification forwarding", timing: "product", status: "active", outcome: null },
      ],
    },
    {
      sprint: 114,
      improvements: [
        { action: "External code reviews (Gemini Code Assist, Copilot) identify real issues before merge - integrate AI review tools as part of Definition of Done for complex PRs", timing: "sprint", status: "completed", outcome: "Gemini Code Assist identified $/progress notification regression (PBI-145) - prioritized as high priority fix and completed in Sprint 115" },
        { action: "Removing #[allow(dead_code)] reveals unused fields and forces proper resource management - Drop implementation added for TokioAsyncBridgeConnection to use reader_handle/shutdown_tx", timing: "immediate", status: "completed", outcome: "Proper Drop impl joins reader task and sends shutdown signal; no resource leaks" },
        { action: "User story benefit should be user-centric not technical - refined mid-sprint from 'async pool sends proper shutdown' to 'diagnostics, hover, and other LSP features stay responsive during language server initialization'", timing: "immediate", status: "completed", outcome: "PBI-144 benefit now describes user value, not implementation detail" },
        { action: "Two flaky tests related to rust-analyzer contention still exist - investigate and fix: tests may need better isolation or longer timeouts for CI environment", timing: "sprint", status: "active", outcome: "Carried forward to Sprint 115 - still needs investigation" },
        { action: "Copilot review Issue #4 (missing error logging when hover=None) - add tracing::debug! or tracing::warn! when bridged hover returns None for observability", timing: "sprint", status: "active", outcome: null },
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
