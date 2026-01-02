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
        role: "developer editing Lua files",
        capability: "run E2E tests for go-to-definition in pure Rust without Neovim dependency",
        benefit: "E2E tests are faster, more reliable, and use snapshot testing for intuitive expected value management",
      },
      acceptance_criteria: [
        {
          criterion: "Rust E2E test infrastructure communicates with treesitter-ls binary via LSP protocol (initialize, textDocument/didOpen, shutdown)",
          verification: "Test helper spawns treesitter-ls process and successfully completes LSP handshake",
        },
        {
          criterion: "textDocument/definition request returns location list through direct binary communication",
          verification: "E2E test sends definition request to treesitter-ls, receives LocationLink or Location response",
        },
        {
          criterion: "Snapshot testing (insta crate) captures expected location responses for go-to-definition",
          verification: "cargo insta test generates/verifies snapshot files for definition responses",
        },
        {
          criterion: "Rust E2E test results match existing Neovim E2E test behavior for go-to-definition",
          verification: "Same test scenario (Lua code block definition lookup) produces equivalent results in both test approaches",
        },
      ],
      status: "ready",
    },
  ],

  sprint: {
    number: 119,
    pbi_id: "PBI-150",
    goal: "Implement Rust-based E2E testing infrastructure for go-to-definition with snapshot testing, enabling faster and more reliable tests without Neovim dependency",
    status: "review",
    subtasks: [
      {
        test: "Write integration test that spawns treesitter-ls binary and verifies process starts (basic spawn test)",
        implementation: "Add insta dev-dependency to Cargo.toml, create tests/e2e_definition.rs with minimal test that spawns binary and checks exit on stdin close",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "17b5293", message: "feat(test): add pure Rust E2E tests for go-to-definition (PBI-150)", phase: "green" }],
        notes: ["Binary already works - test passes immediately (infrastructure setup)"],
      },
      {
        test: "Write test that sends LSP initialize request and expects InitializeResult response with capabilities",
        implementation: "Add LSP protocol helpers (send_request, receive_response) using serde_json for Content-Length framing",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "17b5293", message: "feat(test): add pure Rust E2E tests for go-to-definition (PBI-150)", phase: "green" }],
        notes: ["TDD Red: First attempt received window/logMessage notification instead", "TDD Green: Modified receive_response to skip notifications and wait for actual response"],
      },
      {
        test: "Write test that sends textDocument/didOpen notification after initialize handshake",
        implementation: "Add didOpen notification helper, create test fixture markdown file with Lua code block",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "17b5293", message: "feat(test): add pure Rust E2E tests for go-to-definition (PBI-150)", phase: "green" }],
        notes: ["Test passes immediately - infrastructure already supports notifications"],
      },
      {
        test: "Write test that sends textDocument/definition request and expects LocationLink or Location response",
        implementation: "Add definition request helper, parse GotoDefinitionResponse variants",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "17b5293", message: "feat(test): add pure Rust E2E tests for go-to-definition (PBI-150)", phase: "green" }],
        notes: ["TDD Red: First attempts failed - needed initializationOptions for bridge config", "TDD Red: Also needed receive_response_for_id to skip server-to-client requests", "TDD Green: Added retry logic and initializationOptions matching minimal_init.lua"],
      },
      {
        test: "Write test that captures definition response as insta snapshot for intuitive expected value management",
        implementation: "Add insta::assert_json_snapshot! for definition response, create first snapshot file",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "17b5293", message: "feat(test): add pure Rust E2E tests for go-to-definition (PBI-150)", phase: "green" }],
        notes: ["Added sanitize_definition_response to replace temp URIs with stable placeholder", "Snapshot captures Location array with line 3 (fn example definition)"],
      },
      {
        test: "Write test that verifies Rust E2E produces equivalent results to Neovim E2E (same Lua code block scenario)",
        implementation: "Create matching test fixture (Lua function definition + call), assert line numbers match Neovim test expectations",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "17b5293", message: "feat(test): add pure Rust E2E tests for go-to-definition (PBI-150)", phase: "green" }],
        notes: ["Actually uses Rust code block (not Lua) same as Neovim test", "Verified: cursor line 8 (0-indexed) -> definition line 3 (0-indexed) matches Neovim line 9 -> line 4 (1-indexed)"],
      },
      {
        test: "Write test that sends shutdown request and verifies clean server termination",
        implementation: "Add shutdown/exit sequence to test teardown, verify process exits cleanly",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "17b5293", message: "feat(test): add pure Rust E2E tests for go-to-definition (PBI-150)", phase: "green" }],
        notes: ["TDD Red: shutdown needs no params (modified send_request/send_notification)", "TDD Red: exit notification alone didn't exit - needed stdin close", "TDD Green: stdin=None after exit triggers server process exit"],
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
    { number: 119, pbi_id: "PBI-150", goal: "Skip unsupported languages during auto-install by checking nvim-treesitter metadata before attempting installation, with cached metadata to avoid repeated HTTP requests", status: "done", subtasks: [] },
    { number: 118, pbi_id: "PBI-147", goal: "Return an informative 'No result or indexing' message when bridged hover has no result, ensuring users understand the reason instead of seeing silent empty responses", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-117: modular refactoring pattern, E2E indexing waits, vertical slice validation
  retrospectives: [
    { sprint: 119, improvements: [
      { action: "Reuse existing caching and metadata infrastructure patterns for new features - FetchOptions with TTL proved effective for nvim-treesitter metadata", timing: "immediate", status: "active", outcome: null },
      { action: "Pre-existing E2E test failures should block sprint planning - 12 failing bridge tests indicate deferred technical debt", timing: "sprint", status: "active", outcome: null },
      { action: "Design integration test acceptance criteria with specific verification targets - 'existing tests pass' led to no-op subtask", timing: "sprint", status: "active", outcome: null },
      { action: "User experience issues (noisy errors) should trigger immediate investigation - waiting until PBI-150 suggests reactive rather than proactive approach", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 118, improvements: [
      { action: "Prefer simple user-facing feedback over complex state management - 'No result or indexing' message vs $/progress tracking", timing: "sprint", status: "active", outcome: null },
      { action: "When reverting features, analyze root cause before re-attempting - previous async approach was 'too buggy' due to state complexity", timing: "sprint", status: "active", outcome: null },
      { action: "Helper functions enable testability - create_no_result_hover() testable in isolation", timing: "immediate", status: "completed", outcome: "pub(crate) fn create_no_result_hover() with unit test verification" },
      { action: "Course corrections are valid sprint outcomes - simpler approach after revert delivered user value", timing: "immediate", status: "completed", outcome: "PBI-147 completed with informative message instead of complex indexing state" },
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
