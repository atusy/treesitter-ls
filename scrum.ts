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
    // Completed: PBI-144 (Sprint 114), PBI-145 (Sprint 115), PBI-148 (Sprint 116), PBI-146 (Sprint 117)
    // Rejected: PBI-147 (wait for indexing) - replaced by PBI-149 (informative message approach)
    {
      id: "PBI-149",
      story: {
        role: "Rustacean editing Markdown",
        capability: "see informative message when hover fails due to server indexing",
        benefit: "I understand why hover isn't working and know I can retry later",
      },
      acceptance_criteria: [
        {
          criterion: "TokioAsyncLanguageServerPool tracks ServerState enum (Indexing/Ready) per connection, starting in Indexing state after spawn",
          verification: "Unit test: new connection starts with state Indexing",
        },
        {
          criterion: "hover_impl returns '{ contents: \"⏳ indexing (rust-analyzer)\" }' when ServerState is Indexing",
          verification: "Unit test: hover request with Indexing state returns informative message",
        },
        {
          criterion: "ServerState transitions from Indexing to Ready after first non-empty hover or completion response",
          verification: "Unit test: verify state transition on non-empty response; empty responses keep Indexing state",
        },
        {
          criterion: "Other LSP features (completion, signatureHelp, definition, references) return empty/null during Indexing without special message",
          verification: "Unit test: completion returns [], definition returns null during Indexing state",
        },
        {
          criterion: "End-to-end flow works: hover during indexing shows message, hover after Ready shows normal content",
          verification: "E2E test: trigger hover immediately after server spawn (verify message), wait and retry (verify normal hover)",
        },
      ],
      status: "done",
    },
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
    number: 118,
    pbi_id: "PBI-149",
    goal: "Show informative 'indexing' message during hover when rust-analyzer is still initializing, with state tracking to transition to normal responses once ready",
    status: "done",
    subtasks: [
      {
        test: "Unit test: new connection starts with state Indexing",
        implementation: "Add ServerState enum (Indexing/Ready) and server_states: DashMap<String, ServerState> to TokioAsyncLanguageServerPool, initialize to Indexing after spawn",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "3d6d0e9", message: "feat(bridge): add ServerState enum for tracking indexing state", phase: "green" }],
        notes: ["AC1: TokioAsyncLanguageServerPool tracks ServerState enum per connection"],
      },
      {
        test: "Unit test: hover request with Indexing state returns informative message with hourglass emoji and server name",
        implementation: "Modify hover_impl to check pool.get_server_state(key) and return Hover with indexing message when state is Indexing",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "c22d896", message: "feat(bridge): return informative hover message during indexing", phase: "green" }],
        notes: ["AC2: hover_impl returns informative message during Indexing state"],
      },
      {
        test: "Unit test: verify state transition on non-empty response; empty responses keep Indexing state",
        implementation: "After hover() returns Some(Hover) with non-empty contents, call pool.set_server_state(key, Ready); do not transition on None or empty",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "b5585f2", message: "feat(bridge): transition to Ready state on non-empty hover response", phase: "green" }],
        notes: ["AC3: ServerState transitions from Indexing to Ready on first non-empty hover/completion response"],
      },
      {
        test: "Unit test: completion returns [], definition returns null during Indexing state",
        implementation: "Other LSP features check state and return empty/null during Indexing without special message (only hover shows message)",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "a6f18bc", message: "docs(bridge): add test documenting state tracking for future LSP features", phase: "green" }],
        notes: ["AC4: Other LSP features return empty/null during Indexing without special message"],
      },
      {
        test: "E2E test: trigger hover immediately after server spawn (verify message), wait and retry (verify normal hover)",
        implementation: "Add test_lsp_hover_indexing.lua that verifies end-to-end flow: indexing message shown initially, normal hover after Ready",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "b8b18dd", message: "test(e2e): add hover indexing E2E test and fix existing tests", phase: "green" }],
        notes: ["AC5: End-to-end flow works: hover during indexing shows message, hover after Ready shows normal content"],
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

  // Historical sprints (recent 2) | Sprint 1-116: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 117, pbi_id: "PBI-146", goal: "Track document versions per virtual URI, send didOpen on first access and didChange with incremented version on subsequent accesses, ensuring hover responses reflect the latest code", status: "done", subtasks: [] },
    { number: 116, pbi_id: "PBI-148", goal: "Prevent resource leaks by storing Child handle and temp_dir, sending proper LSP shutdown sequence on drop, and cleaning up temporary workspace", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-115: modular refactoring pattern, E2E indexing waits, vertical slice validation
  retrospectives: [
    { sprint: 117, improvements: [
      { action: "Study reference implementation patterns before new features - sync bridge had versioning model", timing: "sprint", status: "active", outcome: null },
      { action: "DashMap provides thread-safe state without explicit locking - prefer for concurrent access patterns", timing: "immediate", status: "completed", outcome: "document_versions: DashMap<String, u32> in TokioAsyncLanguageServerPool" },
      { action: "LSP spec: didOpen once per URI, didChange for updates with incrementing version", timing: "immediate", status: "completed", outcome: "sync_document checks version map, sends didOpen v1 or didChange v+1" },
      { action: "Tightly coupled changes belong in single commit - all 4 subtasks shared c2a78c0", timing: "immediate", status: "completed", outcome: "fix(bridge): track document versions per URI, send didOpen/didChange correctly" },
    ] },
    { sprint: 116, improvements: [
      { action: "review.md caught resource leaks before production; continue for complex PRs", timing: "sprint", status: "completed", outcome: "PBI-148 fixed process/temp_dir leaks with proper RAII" },
      { action: "Store resource handles in struct from spawn - essential for RAII cleanup", timing: "immediate", status: "completed", outcome: "child: Option<Child>, temp_dir: Option<PathBuf>" },
      { action: "Async shutdown() alongside sync Drop for graceful cleanup when needed", timing: "immediate", status: "completed", outcome: "shutdown() sends LSP exit; Drop kills+removes sync" },
      { action: "E2E test /tmp cleanup as standard for resource cleanup PBIs", timing: "product", status: "active", outcome: null },
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
