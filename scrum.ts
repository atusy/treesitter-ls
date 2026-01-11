// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Lua developer editing markdown",
] as const satisfies readonly string[]; // Must have at least one role. Avoid generic roles like "user" or "admin". Remove obsolete roles freely.

const scrum: ScrumDashboard = {
  product_goal: {
    statement:
      "Implement LSP bridge to support essential language server features indirectly through bridging (ADR-0013, 0014, 0015, 0016, 0017, 0018)",
    success_metrics: [
      {
        metric: "ADR alignment",
        target:
          "Must align with Phase 1 of ADR-0013, 0014, 0015, 0016, 0017, 0018 in @docs/adr",
      },
      {
        metric: "Bridge coverage",
        target:
          "Support completion, signatureHelp, codeAction, definition, hover",
      },
      {
        metric: "Modular architecture",
        target:
          "Bridge module organized with text_document/ subdirectory matching lsp_impl structure",
      },
      {
        metric: "E2E test coverage",
        target: "Each bridged feature has E2E test verifying end-to-end flow",
      },
    ],
  },

  product_backlog: [
    {
      id: "PBI-INIT-TIMEOUT",
      story: {
        role: "Lua developer editing markdown",
        capability: "have the language server respond with a timeout error when lua-language-server fails to initialize within 30 seconds",
        benefit: "I am not stuck waiting indefinitely when the downstream language server hangs during startup",
      },
      acceptance_criteria: [
        { criterion: "Initialization timeout triggers after 30 seconds", verification: "Unit test with tokio::time::timeout wrapping init loop" },
        { criterion: "Timeout returns io::Error propagated to LSP response", verification: "E2E test: verify error response within 35s" },
        { criterion: "Connection not cached after timeout (Failed state)", verification: "Unit test: pool.connections empty after timeout" },
        { criterion: "Timeout duration is configurable constant", verification: "Code review: const INIT_TIMEOUT_SECS = 30" },
      ],
      status: "ready",
      refinement_notes: ["Wrap pool.rs:112-125 with tokio::time::timeout", "ADR-0014/0018 Tier 0: 30-60s"],
    },
    {
      id: "PBI-REQUEST-FAILED-INIT",
      story: {
        role: "Lua developer editing markdown",
        capability: "receive an immediate error response when requesting hover/completion during downstream server initialization",
        benefit: "I get fast feedback instead of the editor appearing frozen",
      },
      acceptance_criteria: [
        { criterion: "Requests during Initializing return REQUEST_FAILED (-32803)", verification: "E2E test with error code check" },
        { criterion: "Error message is 'bridge: downstream server initializing'", verification: "E2E test message verification" },
        { criterion: "ConnectionState enum with Initializing/Ready/Failed", verification: "grep enum ConnectionState src/lsp/bridge/" },
        { criterion: "Requests succeed after Ready state", verification: "E2E test: request after init completes" },
      ],
      status: "ready",
      refinement_notes: ["Depends on PBI-INIT-TIMEOUT", "ADR-0015 Operation Gating"],
    },
  ],
  sprint: {
    number: 151,
    pbi_id: "PBI-INIT-TIMEOUT",
    goal: "Add timeout to initialization to prevent infinite hang when downstream server is unresponsive",
    status: "planning",
    subtasks: [
      {
        test: "Unit test that verifies timeout fires after configured duration (tokio::time::timeout wraps init loop)",
        implementation: "Add const INIT_TIMEOUT_SECS: u64 = 30 and wrap loop at pool.rs:112-125 with tokio::time::timeout",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["ADR-0018 Tier 0: 30-60s recommended", "Target file: src/lsp/bridge/pool.rs lines 109-125"],
      },
      {
        test: "Unit test that timeout returns io::Error with io::ErrorKind::TimedOut",
        implementation: "Map tokio::time::error::Elapsed to io::Error::new(io::ErrorKind::TimedOut, 'Initialize timeout: downstream server unresponsive')",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["LSP compliant: explicit error response, not silent hang"],
      },
      {
        test: "Unit test that pool.connections does not contain entry after timeout (Failed state behavior)",
        implementation: "Only insert into connections HashMap after successful init (current code already does this - verify behavior)",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Connection not cached on timeout ensures retry on next request", "ADR-0015 connection state machine"],
      },
    ],
  },
  completed: [],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
  retrospectives: [],
};

// Type Definitions (DO NOT MODIFY) =============================================
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
  refinement_notes?: string[];
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
