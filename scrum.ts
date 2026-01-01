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

  // Completed PBIs: PBI-001 through PBI-142 (Sprint 1-115) | History: git log -- scrum.yaml, scrum.ts
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  product_backlog: [
    // ADR-0009 Implementation: Vertical slices with user-facing value
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
    number: 116,
    pbi_id: "PBI-143",
    goal: "Implement TokioAsyncLanguageServerPool.signature_help() with async I/O and wire into signature_help_impl to replace spawn_blocking pattern for faster signatureHelp in Markdown code blocks",
    status: "in_progress",
    subtasks: [
      {
        test: "Unit test verifies signature_help() returns SignatureHelp from lua-language-server",
        implementation: "Add signature_help() method to TokioAsyncLanguageServerPool following hover/goto_definition/completion pattern",
        type: "behavioral",
        status: "completed",
        commits: [
          { hash: "5699f03", message: "test(bridge): add signature_help test for TokioAsyncLanguageServerPool", phase: "green" },
          { hash: "c1fca04", message: "feat(bridge): add signature_help() to TokioAsyncLanguageServerPool", phase: "green" },
        ],
        notes: ["Template: completion() method in tokio_async_pool.rs", "Use lua-language-server for faster test (like goto_definition test)"],
      },
      {
        test: "Grep confirms async signature_help path in signature_help_impl.rs",
        implementation: "Replace spawn_blocking with tokio_async_pool.signature_help() call in signature_help_impl",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Template: hover_impl.rs pattern", "Remove language_server_pool.take_connection call", "Use self.tokio_async_pool.signature_help()"],
      },
      {
        test: "E2E test opens Markdown with Rust code block, requests signatureHelp, receives signatures",
        implementation: "Update test_lsp_signature_help.lua with retry loop for rust-analyzer indexing",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Existing test has 3s sleep - add retry loop like hover/completion tests", "Pattern: 20 iterations with 500ms sleep"],
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

  // Historical sprints (recent 2) | Sprint 1-114: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 115, pbi_id: "PBI-142", goal: "Implement TokioAsyncLanguageServerPool.completion() with async I/O and wire into completion_impl to replace spawn_blocking pattern for faster completion in Markdown code blocks", status: "done", subtasks: [] },
    { number: 114, pbi_id: "PBI-141", goal: "Implement TokioAsyncLanguageServerPool.goto_definition() with async I/O and wire into definition_impl to replace spawn_blocking pattern for faster go-to-definition in Markdown code blocks", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-113: modular refactoring pattern, E2E indexing waits
  retrospectives: [
    {
      sprint: 115,
      improvements: [
        { action: "Template pattern (hover -> goto_definition -> completion) working smoothly - each new feature follows established pattern with minimal friction", timing: "immediate", status: "completed", outcome: "completion() implemented by following goto_definition() as template; pattern now proven across 3 features" },
        { action: "E2E test already existed for completion, just needed retry loop for rust-analyzer indexing - consistent pattern emerging across E2E tests", timing: "immediate", status: "completed", outcome: "E2E test passes with 20-iteration retry loop; same pattern as hover test" },
        { action: "Flaky test in sync bridge (did_open_uses_connection_info_write_virtual_file) still present - passes with --test-threads=1, fails intermittently with parallel execution", timing: "sprint", status: "active", outcome: null },
        { action: "Continue pattern for PBI-143 (signatureHelp) - last remaining async bridge PBI; follow established template", timing: "sprint", status: "active", outcome: null },
        { action: "Consider adding shared E2E test helper for retry pattern - duplicate retry logic across hover/completion tests could be extracted", timing: "product", status: "active", outcome: null },
      ],
    },
    {
      sprint: 114,
      improvements: [
        { action: "Vertical slice pattern continued successfully - PBI-141 (goto_definition) followed same pattern as PBI-140 (hover), confirming the template approach works well for bridge feature expansion", timing: "immediate", status: "completed", outcome: "goto_definition implemented by following hover() as template, demonstrating pattern reusability" },
        { action: "Addressed Sprint 113 action: removed #[allow(dead_code)] annotations from TokioAsyncLanguageServerPool now that hover() and goto_definition() are wired into production", timing: "immediate", status: "completed", outcome: "Commit 58390b3 - kept dead_code only on has_connection() and notification_sender() (test-only or not yet used)" },
        { action: "Discovered cwd issue for rust-analyzer - language servers may need cwd set to workspace root; added spawn_with_cwd() to TokioAsyncBridgeConnection", timing: "immediate", status: "completed", outcome: "E2E test now passes with proper cwd handling for rust-analyzer" },
        { action: "Fix outdated comment in definition.rs (line 170) - says 'spawn_blocking' but code now uses async pool.goto_definition(); similar comments in inlay_hint.rs, implementation.rs also need update when those migrate to async", timing: "sprint", status: "active", outcome: null },
        { action: "Continue pattern for PBI-142 (completion) and PBI-143 (signatureHelp) - follow hover/goto_definition template for consistent implementation", timing: "sprint", status: "completed", outcome: "Completed PBI-142 (completion) in Sprint 115 following same pattern" },
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
