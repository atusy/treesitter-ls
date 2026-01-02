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

  // Completed PBIs: PBI-001 through PBI-151 (Sprint 1-120) | History: git log -- scrum.yaml, scrum.ts
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  product_backlog: [
    // ADR-0009 Implementation: Vertical slices with user-facing value
    // Completed: PBI-144 (S114), PBI-145 (S115), PBI-148 (S116), PBI-146 (S117), PBI-149 (S118), PBI-150 (S119), PBI-151 (S120)
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
      status: "done",
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
      status: "done",
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
    number: 121,
    pbi_id: "PBI-143",
    goal: "Implement async signatureHelp for Markdown code blocks, enabling signature help requests to use fully async I/O without blocking other LSP requests, with full E2E_TEST_CHECKLIST.md compliance from the start",
    status: "in_progress",
    subtasks: [
      {
        test: "Unit test in tokio_async_pool.rs verifies pool.signature_help() returns Some(SignatureHelp) from rust-analyzer",
        implementation: "Add signature_help() method to TokioAsyncLanguageServerPool following completion() pattern (async request/response with 30s timeout)",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "7983721",
            message: "feat(async): add signature_help() method to TokioAsyncLanguageServerPool",
            phase: "green",
          },
        ],
        notes: [
          "Pattern: Copy completion() method (lines 326-368 in tokio_async_pool.rs)",
          "Change request method to 'textDocument/signatureHelp'",
          "Change return type to tower_lsp::lsp_types::SignatureHelp",
          "Follow same sync_document -> send_request -> await response pattern",
        ],
      },
      {
        test: "Grep verification confirms signature_help.rs uses async pool.signature_help() (no spawn_blocking)",
        implementation: "Refactor signature_help handler to use tokio_async_pool.signature_help() instead of spawn_blocking path",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "1e77161",
            message: "refactor(async): use tokio_async_pool.signature_help() in handler",
            phase: "green",
          },
        ],
        notes: [
          "Replace spawn_blocking call (line 137) with async pool.signature_help()",
          "Remove manual notification forwarding (lines 165-175) - async pool handles it",
          "Follow hover handler refactoring pattern from Sprint 118",
        ],
      },
      {
        test: "Checklist review documented in commit message (Pre-implementation step from Sprint 120 Retrospective)",
        implementation: "Review E2E_TEST_CHECKLIST.md and existing test helpers before writing test code",
        type: "structural",
        status: "completed",
        commits: [
          {
            hash: "pending",
            message: "docs(scrum): complete pre-implementation E2E checklist review",
            phase: "green",
          },
        ],
        notes: [
          "CRITICAL: This is the Sprint 120 Retrospective action - must be done BEFORE Subtask 4",
          "Review scripts/minimal_init.lua for helper.retry_for_lsp_indexing() usage",
          "Review test_lsp_hover.lua and test_lsp_completion.lua for established patterns",
          "Identify violations in current test_lsp_signature_help.lua (manual sleep, weak assertions)",
          "FINDINGS: 1) helper.retry_for_lsp_indexing() exists (lines 91-119 minimal_init.lua) but NO tests use it yet, 2) test_lsp_hover.lua has manual retry loop (lines 59-81), 3) test_lsp_signature_help.lua has vim.uv.sleep(3000) manual wait (line 55) and weak assertions (lines 98-121), 4) E2E_TEST_CHECKLIST.md lines 15-38 specify REQUIRED use of helper, DO NOT use manual loops",
        ],
      },
      {
        test: "E2E test uses helper.retry_for_lsp_indexing() and verifies signature help returns valid response for function call",
        implementation: "Refactor test_lsp_signature_help.lua to use retry helper instead of manual sleep, strengthen assertions",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "MUST use _G.helper.retry_for_lsp_indexing() from scripts/minimal_init.lua",
          "Remove vim.uv.sleep(3000) and manual retry loop (current line 55)",
          "Verify specific signature details: label contains 'add', parameter info present",
          "Follow E2E_TEST_CHECKLIST.md mandatory patterns (lines 15-38)",
        ],
      },
      {
        test: "E2E test 'markdown_rust_async_signature_help' verifies async path with realistic scenario showing parameter hints",
        implementation: "Add dedicated async path E2E test that exercises full signature_help request/response cycle",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Test name must have '_async' suffix per E2E_TEST_CHECKLIST.md",
          "Realistic scenario: Function with multiple parameters, verify activeParameter tracking",
          "Strong assertions: Verify signature.parameters array, activeParameter index",
          "Use helper.retry_for_lsp_indexing() for resilience",
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

  // Historical sprints (recent 2) | Sprint 1-120: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 120, pbi_id: "PBI-151", goal: "Migrate critical Neovim E2E tests (hover, completion, references) to Rust with snapshot verification, establishing reusable patterns and helpers for future migrations", status: "done", subtasks: [] },
    { number: 119, pbi_id: "PBI-150", goal: "Implement Rust-based E2E testing infrastructure for go-to-definition with snapshot testing, enabling faster and more reliable tests without Neovim dependency", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-120: ADR-driven development, reusable patterns, E2E test timing
  retrospectives: [
    { sprint: 120, improvements: [
      { action: "Plan helper module architecture during sprint planning - identify reusable abstractions (LspClient, test fixtures, initialization patterns) before implementation starts to avoid mid-sprint extraction", timing: "sprint", status: "active", outcome: null },
      { action: "Document snapshot testing sanitization patterns - create testing guide explaining URI replacement, range normalization, non-deterministic data handling with examples from hover/completion/references tests", timing: "sprint", status: "active", outcome: null },
      { action: "Apply LSP spec study to test design - even for test migrations, studying textDocument/hover, textDocument/completion, textDocument/references spec helps identify edge cases and sanitization needs upfront", timing: "immediate", status: "active", outcome: null },
      { action: "Consider extracting E2E test helpers into shared testing library - tests/helpers_*.rs modules (lsp_client, lsp_polling, sanitization, fixtures) could become reusable crate if more LSP features will be tested", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 119, improvements: [
      { action: "Study LSP specification sections before implementing new LSP features - JSON-RPC 2.0, notification vs request semantics, server-initiated requests", timing: "immediate", status: "active", outcome: null },
      { action: "Extract retry-with-timeout pattern into reusable test helper - poll_until or wait_for_lsp_response with configurable attempts/delay", timing: "immediate", status: "completed", outcome: "poll_until(max_attempts, delay_ms, predicate) helper created in tests/helpers_lsp_polling.rs (Sprint 120 subtask 1-2)" },
      { action: "Document snapshot testing best practices - sanitization strategies for non-deterministic data (temp paths, timestamps, PIDs)", timing: "sprint", status: "active", outcome: null },
      { action: "Establish E2E testing strategy guidelines - when to use Rust E2E (protocol verification, CI speed) vs Neovim E2E (editor integration, user workflow)", timing: "sprint", status: "active", outcome: null },
      { action: "Consider migrating critical Neovim E2E tests to Rust - evaluate hover, completion for snapshot testing benefits", timing: "product", status: "completed", outcome: "Sprint 120 successfully migrated hover, completion, references with snapshot verification - pattern proven effective" },
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
