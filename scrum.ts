// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Rustacean editing Markdown",
  "developer editing Lua files",
  "documentation author with Rust code blocks",
  "treesitter-ls user managing configurations",
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

  // Completed PBIs: PBI-001 through PBI-140 (Sprint 1-113), PBI-155 (Sprint 124), PBI-156 (Sprint 125) | History: git log -- scrum.yaml, scrum.ts
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  product_backlog: [
    {
      id: "PBI-156",
      story: {
        role: "treesitter-ls user managing configurations",
        capability: "configure languages using `parser` and unified `queries` fields",
        benefit: "configuration schema is consistent and simpler with fewer redundant fields",
      },
      acceptance_criteria: [
        {
          criterion: "LanguageSettings uses `parser` field instead of `library`",
          verification: "grep confirms no `library` field in LanguageSettings struct",
        },
        {
          criterion: "LanguageSettings uses unified `queries: Vec<QueryItem>` instead of separate highlights/locals/injections",
          verification: "grep confirms no highlights/locals/injections fields in LanguageSettings struct",
        },
        {
          criterion: "resolve_language_settings_with_wildcard merges the new schema correctly",
          verification: "Unit tests verify parser and queries fields merge with wildcard inheritance",
        },
      ],
      status: "done",
    },
    // ADR-0009 Implementation: Vertical slices with user-facing value
    // Completed: PBI-144 (Sprint 114), PBI-145 (Sprint 115), PBI-148 (Sprint 116), PBI-146 (Sprint 117)
    {
      id: "PBI-147",
      story: {
        role: "Rustacean editing Markdown",
        capability: "get hover results on first request without needing to retry",
        benefit: "hover works reliably the first time I trigger it on a new code block",
      },
      acceptance_criteria: [
        {
          criterion: "spawn_and_initialize waits for rust-analyzer to complete initial indexing",
          verification: "Unit test verifies hover is not called until indexing is complete",
        },
        {
          criterion: "Wait uses $/progress notifications to detect indexing completion",
          verification: "Unit test verifies $/progress notifications are monitored and indexing end is detected",
        },
        {
          criterion: "Single hover request returns result without retry loop",
          verification: "E2E test verifies single hover request returns result (no retry loop needed)",
        },
      ],
      status: "ready",
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
    // ADR-0010: Completed PBI-151 (Sprint 118), PBI-150 (Sprint 119), PBI-149 (Sprint 120)
    // ADR-0011: Completed PBI-152 (Sprint 121), PBI-153 (Sprint 122), PBI-154 (Sprint 123), PBI-155 (Sprint 124)
  ],
  sprint: null,
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },
  // Historical sprints (recent 2) | Sprint 1-124: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 125, pbi_id: "PBI-156", goal: "Migrate LanguageSettings to use parser and unified queries fields", status: "done", subtasks: [] },
    { number: 124, pbi_id: "PBI-155", goal: "Wire config APIs into app: user config loading, wildcard resolution", status: "done", subtasks: [] },
  ],
  // Retrospectives (recent 2)
  retrospectives: [
    { sprint: 125, improvements: [
      { action: "Document domain vs serialization type distinction in ADR: LanguageSettings (domain) vs LanguageConfig (serialization) separation rationale", timing: "immediate", status: "active", outcome: null },
      { action: "Consider introducing explicit conversion trait between domain and serialization types for type safety", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 124, improvements: [
      { action: "Always plan integration sprint immediately after infrastructure work to deliver user value", timing: "sprint", status: "active", outcome: null },
      { action: "Document infrastructure-integration pattern in ADR: APIs must be wired into application within 1-2 sprints of creation", timing: "immediate", status: "completed", outcome: "Added to ADR-0010 and ADR-0011 'Consequences' sections" },
      { action: "Consider splitting large infrastructure work: alternate infrastructure + integration sprints for incremental value delivery", timing: "product", status: "active", outcome: null },
    ] },
  ],
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
