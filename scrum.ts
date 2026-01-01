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

  // Completed PBIs: PBI-001 through PBI-141 (Sprint 1-114) | History: git log -- scrum.yaml, scrum.ts
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  product_backlog: [
    // ADR-0009 Implementation: Vertical slices with user-facing value
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

  // Historical sprints (recent 2) | Sprint 1-113: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 114, pbi_id: "PBI-141", goal: "Implement TokioAsyncLanguageServerPool.goto_definition() with async I/O and wire into definition_impl to replace spawn_blocking pattern for faster go-to-definition in Markdown code blocks", status: "done", subtasks: [] },
    { number: 113, pbi_id: "PBI-140", goal: "Implement fully async hover bridging with TokioAsyncBridgeConnection reader task, TokioAsyncLanguageServerPool, and wire into hover_impl to replace spawn_blocking pattern", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-112: modular refactoring pattern, E2E indexing waits
  retrospectives: [
    {
      sprint: 114,
      improvements: [
        { action: "Vertical slice pattern continued successfully - PBI-141 (goto_definition) followed same pattern as PBI-140 (hover), confirming the template approach works well for bridge feature expansion", timing: "immediate", status: "completed", outcome: "goto_definition implemented by following hover() as template, demonstrating pattern reusability" },
        { action: "Addressed Sprint 113 action: removed #[allow(dead_code)] annotations from TokioAsyncLanguageServerPool now that hover() and goto_definition() are wired into production", timing: "immediate", status: "completed", outcome: "Commit 58390b3 - kept dead_code only on has_connection() and notification_sender() (test-only or not yet used)" },
        { action: "Discovered cwd issue for rust-analyzer - language servers may need cwd set to workspace root; added spawn_with_cwd() to TokioAsyncBridgeConnection", timing: "immediate", status: "completed", outcome: "E2E test now passes with proper cwd handling for rust-analyzer" },
        { action: "Fix outdated comment in definition.rs (line 170) - says 'spawn_blocking' but code now uses async pool.goto_definition(); similar comments in inlay_hint.rs, implementation.rs also need update when those migrate to async", timing: "sprint", status: "active", outcome: null },
        { action: "Continue pattern for PBI-142 (completion) and PBI-143 (signatureHelp) - follow hover/goto_definition template for consistent implementation", timing: "sprint", status: "active", outcome: null },
      ],
    },
    {
      sprint: 113,
      improvements: [
        { action: "INVEST-compliant vertical slice pattern validated - PBI-140 delivered user value (faster hover responses) by combining infrastructure (TokioAsyncBridgeConnection), wiring (TokioAsyncLanguageServerPool into TreeSitterLs), and E2E test in single PBI", timing: "immediate", status: "completed", outcome: "Avoided PBI-091 anti-pattern where infrastructure was never wired; async pool is now used in production hover path" },
        { action: "Apply vertical slice pattern to PBI-142 (completion + signatureHelp) - each PBI should deliver observable user value, not just infrastructure changes", timing: "sprint", status: "completed", outcome: "Applied in Sprint 114 with PBI-141 goto_definition" },
        { action: "Sync bridge module has potential flaky test (read_response_for_id_with_notifications_returns_none_on_timeout uses 100ms timeout with 5s assertion slack) - monitor for CI failures, consider increasing timeout margin if flaky", timing: "sprint", status: "active", outcome: null },
        { action: "tokio::select! pattern enables clean async shutdown - reader task completes within 100ms when idle (AC1 verified), unlike sync read_line which blocks until data arrives", timing: "immediate", status: "completed", outcome: "Test shutdown_while_reader_idle_completes_within_100ms passes consistently" },
        { action: "Remove #[allow(dead_code)] from TokioAsyncBridgeConnection and TokioAsyncLanguageServerPool now that they are wired into production code (hover path uses them)", timing: "sprint", status: "completed", outcome: "Completed in Sprint 114 - commit 58390b3" },
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
