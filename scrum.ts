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
    // Completed: PBI-144 (Sprint 114), PBI-145 (Sprint 115), PBI-148 (Sprint 116), PBI-146 (Sprint 117), PBI-147 (Sprint 118)
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
    {
      id: "PBI-150",
      story: {
        role: "documentation author with Rust code blocks",
        capability: "not see noisy error notifications for unsupported languages during auto-install",
        benefit: "I get a cleaner experience without failed installation attempts for languages that treesitter-ls cannot support",
      },
      acceptance_criteria: [
        {
          criterion: "maybe_auto_install_language checks if language exists in nvim-treesitter metadata before attempting installation",
          verification: "Unit test: calling maybe_auto_install_language with unsupported language (e.g., 'fake_lang') skips installation without error notification",
        },
        {
          criterion: "Supported languages list is cached to avoid repeated HTTP requests during a session",
          verification: "Unit test: multiple is_language_supported checks reuse cached metadata (verify single HTTP fetch via mock or cache hit count)",
        },
        {
          criterion: "User receives informative log message when language is skipped due to being unsupported",
          verification: "Unit test: verify log message contains language name and reason (not supported by nvim-treesitter)",
        },
        {
          criterion: "Existing auto-install flow continues to work for supported languages",
          verification: "Integration test: auto-install for 'lua' proceeds normally (existing tests pass)",
        },
      ],
      status: "done",
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

  // Historical sprints (recent 2) | Sprint 1-117: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 119, pbi_id: "PBI-150", goal: "Skip unsupported languages during auto-install by checking nvim-treesitter metadata before attempting installation, with cached metadata to avoid repeated HTTP requests", status: "done", subtasks: [] },
    { number: 118, pbi_id: "PBI-147", goal: "Return an informative 'No result or indexing' message when bridged hover has no result, ensuring users understand the reason instead of seeing silent empty responses", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-116: modular refactoring pattern, E2E indexing waits, vertical slice validation
  retrospectives: [
    { sprint: 118, improvements: [
      { action: "Prefer simple user-facing feedback over complex state management - 'No result or indexing' message vs $/progress tracking", timing: "sprint", status: "active", outcome: null },
      { action: "When reverting features, analyze root cause before re-attempting - previous async approach was 'too buggy' due to state complexity", timing: "sprint", status: "active", outcome: null },
      { action: "Helper functions enable testability - create_no_result_hover() testable in isolation", timing: "immediate", status: "completed", outcome: "pub(crate) fn create_no_result_hover() with unit test verification" },
      { action: "Course corrections are valid sprint outcomes - simpler approach after revert delivered user value", timing: "immediate", status: "completed", outcome: "PBI-147 completed with informative message instead of complex indexing state" },
    ] },
    { sprint: 117, improvements: [
      { action: "Study reference implementation patterns before new features - sync bridge had versioning model", timing: "sprint", status: "active", outcome: null },
      { action: "DashMap provides thread-safe state without explicit locking - prefer for concurrent access patterns", timing: "immediate", status: "completed", outcome: "document_versions: DashMap<String, u32> in TokioAsyncLanguageServerPool" },
      { action: "LSP spec: didOpen once per URI, didChange for updates with incrementing version", timing: "immediate", status: "completed", outcome: "sync_document checks version map, sends didOpen v1 or didChange v+1" },
      { action: "Tightly coupled changes belong in single commit - all 4 subtasks shared c2a78c0", timing: "immediate", status: "completed", outcome: "fix(bridge): track document versions per URI, send didOpen/didChange correctly" },
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
