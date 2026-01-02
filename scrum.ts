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

  // Completed PBIs: PBI-001 through PBI-150 (Sprint 1-119) | History: git log -- scrum.yaml, scrum.ts
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  product_backlog: [
    // ADR-0009 Implementation: Vertical slices with user-facing value
    // Completed: PBI-144 (S114), PBI-145 (S115), PBI-148 (S116), PBI-146 (S117), PBI-149 (S118), PBI-150 (S119)
    // Rejected: PBI-147 (wait for indexing) - replaced by PBI-149 (informative message approach)
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
    number: 120,
    pbi_id: "PBI-151",
    goal: "Migrate critical Neovim E2E tests (hover, completion, references) to Rust with snapshot verification, establishing reusable patterns and helpers for future migrations",
    status: "in_progress",
    subtasks: [
      {
        test: "Extract poll_until helper from e2e_definition.rs retry logic into test helper module",
        implementation: "Create tests/helpers_lsp_polling.rs with poll_until(max_attempts, delay_ms, predicate) function that encapsulates retry-with-timeout pattern",
        type: "structural",
        status: "completed",
        commits: [
          {
            hash: "f187c14",
            message: "refactor(test): extract poll_until helper for E2E test retry logic",
            phase: "refactoring",
          },
        ],
        notes: [
          "Addresses Sprint 119 retrospective: Extract retry-with-timeout pattern into reusable test helper",
          "This is structural refactoring to prepare for test migrations",
        ],
      },
      {
        test: "Refactor e2e_definition.rs tests to use poll_until helper instead of manual retry loops",
        implementation: "Replace the 20-attempt retry loops in e2e_definition.rs with calls to poll_until helper",
        type: "structural",
        status: "completed",
        commits: [
          {
            hash: "5196299",
            message: "refactor(test): use poll_until helper in e2e_definition tests",
            phase: "refactoring",
          },
        ],
        notes: [
          "Validates the poll_until abstraction works for existing definition tests",
          "This refactoring proves the helper is reusable before writing new tests",
        ],
      },
      {
        test: "Write test_hover_returns_content Rust E2E test that sends hover request at cursor position and validates response structure",
        implementation: "Implement hover E2E test using LspClient to send textDocument/hover request, poll_until for response, assert hover content exists",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "2ebdc1c",
            message: "test(e2e): migrate hover test from Lua to Rust",
            phase: "green",
          },
        ],
        notes: [
          "Migrates from tests/test_lsp_hover.lua",
          "Cursor on 'main' in fn main() at line 4, column 4",
          "Accepts either real hover content or 'indexing (rust-analyzer)' message (PBI-149)",
        ],
      },
      {
        test: "Add sanitization helper for hover responses that replaces non-deterministic data (file URIs, markdown formatting variations)",
        implementation: "Create sanitize_hover_response function in tests/helpers_sanitization.rs to normalize hover content for snapshot comparison",
        type: "structural",
        status: "completed",
        commits: [
          {
            hash: "3e1b7d0",
            message: "refactor(test): add sanitization helper for hover responses",
            phase: "refactoring",
          },
        ],
        notes: [
          "Structural refactoring to support snapshot testing",
          "Prepares for hover snapshot test in next subtask",
        ],
      },
      {
        test: "Write test_hover_snapshot test that captures sanitized hover response in insta snapshot",
        implementation: "Add snapshot test using insta::assert_json_snapshot with sanitized hover response",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "6ef5e56",
            message: "test(e2e): add hover snapshot test with deterministic verification",
            phase: "green",
          },
        ],
        notes: [
          "Verifies hover content is deterministic and stable across runs",
          "Documents expected hover response structure",
        ],
      },
      {
        test: "Extract LspClient into shared test helper module for reuse across E2E test files",
        implementation: "Move LspClient struct and implementation from e2e_definition.rs to tests/helpers_lsp_client.rs, export as pub",
        type: "structural",
        status: "pending",
        commits: [],
        notes: [
          "Addresses PBI-151 acceptance criterion: LspClient helper in reusable module",
          "Enables upcoming completion and references tests to reuse LspClient",
        ],
      },
      {
        test: "Write test_completion_returns_items Rust E2E test that requests completion after 'p.' and validates struct field items",
        implementation: "Implement completion E2E test using shared LspClient, poll_until, verify completion items include 'x' and 'y' fields",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Migrates from tests/test_lsp_completion.lua",
          "Cursor after 'p.' on line 11 inside Rust code block",
          "Verifies textEdit ranges are in host document coordinates (line >= 10)",
        ],
      },
      {
        test: "Refactor completion test to extract common initialization pattern into helper function",
        implementation: "Create initialize_with_bridge helper that encapsulates initialize + initialized + bridge config setup",
        type: "structural",
        status: "pending",
        commits: [],
        notes: [
          "Reduces duplication across definition, hover, and completion tests",
          "Prepares clean foundation for references test",
        ],
      },
      {
        test: "Write test_completion_snapshot test that captures sanitized completion items in snapshot",
        implementation: "Add snapshot test for completion response with sanitized textEdit ranges",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Verifies completion items are stable and deterministic",
          "Validates range adjustment from virtual to host coordinates",
        ],
      },
      {
        test: "Write test_references_returns_locations Rust E2E test that finds all references to variable 'x'",
        implementation: "Implement references E2E test using shared helpers, poll_until, verify 3+ reference locations with host coordinates",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Migrates from tests/test_lsp_references.lua",
          "Cursor on 'x' definition at line 5",
          "Expects references at lines 5, 6, 7 (all >= line 3 in 0-indexed host coordinates)",
        ],
      },
      {
        test: "Write test_references_snapshot test that captures sanitized reference locations",
        implementation: "Add snapshot test for references response with sanitized URIs and stable coordinates",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Completes the TDD cycle for references migration",
          "Establishes pattern: functional test -> refactor helpers -> snapshot test",
        ],
      },
      {
        test: "Refactor E2E test helpers by extracting create_test_markdown_file variants into reusable module",
        implementation: "Create tests/helpers_test_fixtures.rs with functions for hover_fixture, completion_fixture, references_fixture",
        type: "structural",
        status: "pending",
        commits: [],
        notes: [
          "Final refactoring to consolidate common test setup patterns",
          "Prepares clean foundation for future LSP feature migrations (rename, codeAction, etc.)",
          "User requirement: refactor E2E test on every migration cycles",
        ],
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

  // Historical sprints (recent 2) | Sprint 1-118: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 119, pbi_id: "PBI-150", goal: "Implement Rust-based E2E testing infrastructure for go-to-definition with snapshot testing, enabling faster and more reliable tests without Neovim dependency", status: "done", subtasks: [] },
    { number: 118, pbi_id: "PBI-149", goal: "Show informative 'indexing' message during hover when rust-analyzer is still initializing, with state tracking to transition to normal responses once ready", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-117: modular refactoring pattern, E2E indexing waits, vertical slice validation
  retrospectives: [
    { sprint: 119, improvements: [
      { action: "Study LSP specification sections before implementing new LSP features - JSON-RPC 2.0, notification vs request semantics, server-initiated requests", timing: "immediate", status: "active", outcome: null },
      { action: "Extract retry-with-timeout pattern into reusable test helper - poll_until or wait_for_lsp_response with configurable attempts/delay", timing: "immediate", status: "active", outcome: null },
      { action: "Document snapshot testing best practices - sanitization strategies for non-deterministic data (temp paths, timestamps, PIDs)", timing: "sprint", status: "active", outcome: null },
      { action: "Establish E2E testing strategy guidelines - when to use Rust E2E (protocol verification, CI speed) vs Neovim E2E (editor integration, user workflow)", timing: "sprint", status: "active", outcome: null },
      { action: "Consider migrating critical Neovim E2E tests to Rust - evaluate hover, completion for snapshot testing benefits", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 118, improvements: [
      { action: "ADR-driven development accelerates implementation - ADR-0010 pre-defined architecture, state machine, and detection heuristic", timing: "sprint", status: "active", outcome: null },
      { action: "Reusable patterns across sprints reduce cognitive load - DashMap from Sprint 117 enabled consistent state tracking", timing: "immediate", status: "completed", outcome: "server_states: DashMap<String, ServerState> mirrors document_versions pattern" },
      { action: "E2E test retries indicate timing assumptions - 20-attempt loop with 500ms wait works but shows brittleness", timing: "sprint", status: "active", outcome: null },
      { action: "Non-deterministic test assertions reduce reliability - comment 'may or may not see indexing message' shows test unpredictability", timing: "product", status: "active", outcome: null },
      { action: "Feature changes ripple to existing tests - hover test updated for indexing state shows broader impact than anticipated", timing: "sprint", status: "active", outcome: null },
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
