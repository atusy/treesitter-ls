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
    // Completed: PBI-144 (Sprint 114), PBI-145 (Sprint 115), PBI-148 (Sprint 116), PBI-146 (Sprint 117), PBI-149 (Sprint 118)
    // Rejected: PBI-147 (wait for indexing) - replaced by PBI-149
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

  sprint: {
    number: 119,
    pbi_id: "PBI-141",
    goal: "Implement fully async go-to-definition for Lua code blocks in Markdown, eliminating spawn_blocking and enabling concurrent LSP requests",
    status: "review",
    subtasks: [
      {
        test: "Unit test: TokioAsyncLanguageServerPool.goto_definition() returns valid GotoDefinitionResponse",
        implementation: "Add goto_definition() method to TokioAsyncLanguageServerPool with async request/response pattern (sync_document + textDocument/definition request)",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "174115f", message: "feat(bridge): add goto_definition() to TokioAsyncLanguageServerPool", phase: "green" }],
        notes: ["Follow hover() pattern: get_connection, sync_document, send_request, parse response", "Reference: src/lsp/bridge/tokio_async_pool.rs hover() method"],
      },
      {
        test: "Grep verification: no spawn_blocking in definition.rs for bridged requests",
        implementation: "Modify definition_impl to use tokio_async_pool.goto_definition() instead of spawn_blocking with sync pool",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "6a10a3a", message: "feat(bridge): use async pool for goto_definition in definition_impl", phase: "green" }],
        notes: ["Replace take_connection/spawn_blocking pattern with async pool.goto_definition()", "Handle position translation same as current impl", "Reference: current definition.rs lines 200-241"],
      },
      {
        test: "E2E test: Markdown with Lua code block returns definition location through async path",
        implementation: "Create test_lsp_definition_lua.lua following test_lsp_definition.lua pattern with Lua code",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "416882d", message: "test(e2e): add Lua go-to-definition test through async bridge", phase: "green" }],
        notes: ["Use lua-language-server for Lua code blocks", "Test local function definition and reference", "Reference: tests/test_lsp_definition.lua pattern"],
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

  // Historical sprints (recent 2) | Sprint 1-117: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 118, pbi_id: "PBI-149", goal: "Show informative 'indexing' message during hover when rust-analyzer is still initializing, with state tracking to transition to normal responses once ready", status: "done", subtasks: [] },
    { number: 117, pbi_id: "PBI-146", goal: "Track document versions per virtual URI, send didOpen on first access and didChange with incremented version on subsequent accesses, ensuring hover responses reflect the latest code", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-116: modular refactoring pattern, E2E indexing waits, vertical slice validation
  retrospectives: [
    { sprint: 118, improvements: [
      { action: "ADR-driven development accelerates implementation - ADR-0010 pre-defined architecture, state machine, and detection heuristic", timing: "sprint", status: "active", outcome: null },
      { action: "Reusable patterns across sprints reduce cognitive load - DashMap from Sprint 117 enabled consistent state tracking", timing: "immediate", status: "completed", outcome: "server_states: DashMap<String, ServerState> mirrors document_versions pattern" },
      { action: "E2E test retries indicate timing assumptions - 20-attempt loop with 500ms wait works but shows brittleness", timing: "sprint", status: "active", outcome: null },
      { action: "Non-deterministic test assertions reduce reliability - comment 'may or may not see indexing message' shows test unpredictability", timing: "product", status: "active", outcome: null },
      { action: "Feature changes ripple to existing tests - hover test updated for indexing state shows broader impact than anticipated", timing: "sprint", status: "active", outcome: null },
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
