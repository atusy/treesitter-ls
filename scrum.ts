// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Rustacean editing Markdown",
  "developer editing Lua files",
  "documentation author with Rust code blocks",
  "treesitter-ls user managing configurations",
] as const satisfies readonly string[];

const scrum: ScrumDashboard = {
  product_goal: {
    statement:
      "Maintain stable async LSP bridge for core features using single-pool architecture (ADR-0006, 0007, 0008)",
    success_metrics: [
      { metric: "Bridge coverage", target: "Support hover, completion, signatureHelp, definition with fully async implementations" },
      { metric: "Modular architecture", target: "Bridge module organized with text_document/ subdirectory, single TokioAsyncLanguageServerPool" },
      { metric: "E2E test coverage", target: "Each bridged feature has E2E test verifying end-to-end async flow" },
    ],
  },

  // Completed PBIs: PBI-001 through PBI-140 (Sprint 1-113) | History: git log -- scrum.yaml, scrum.ts
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow), PBI-171 ($/cancelRequest - tower-lsp internals)
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
    // ADR-0010 Implementation: Configuration Merging Strategy
    {
      id: "PBI-151",
      story: {
        role: "treesitter-ls user managing configurations",
        capability: "configure query files using a unified queries field with automatic type inference",
        benefit: "I write less configuration and filenames like highlights.scm are self-documenting",
      },
      acceptance_criteria: [
        {
          criterion: "QueryItem struct with path (required) and kind (optional) fields parses correctly",
          verification: "Unit test verifies TOML parsing of queries array with and without kind field",
        },
        {
          criterion: "Type inference: *highlights*.scm -> highlights, *locals*.scm -> locals, *injections*.scm -> injections",
          verification: "Unit test verifies type inference for various filename patterns",
        },
        {
          criterion: "Default kind is highlights when filename has no recognizable pattern",
          verification: "Unit test verifies custom.scm and python.scm default to highlights",
        },
      ],
      status: "ready",
    },
  ],

  sprint: {
    number: 118,
    pbi_id: "PBI-151",
    goal: "Enable unified query configuration with a queries array that infers type from filename patterns (*highlights*.scm, *locals*.scm, *injections*.scm), reducing configuration verbosity",
    status: "done",
    subtasks: [
      {
        test: "Verify QueryItem struct with path (required) and kind (optional) fields parses from TOML correctly",
        implementation: "Define QueryItem struct with serde derives for path: String and kind: Option<QueryKind>",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "d7c430f", message: "feat(config): add QueryItem struct and QueryKind enum for unified query configuration", phase: "green" }],
        notes: [],
      },
      {
        test: "Verify type inference: *highlights*.scm -> highlights, *locals*.scm -> locals, *injections*.scm -> injections",
        implementation: "Implement infer_query_kind() function that matches filename patterns to QueryKind enum",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "a275d04", message: "feat(config): add infer_query_kind function for filename-based type inference", phase: "green" }],
        notes: [],
      },
      {
        test: "Verify custom.scm and python.scm (no recognizable pattern) default to highlights kind",
        implementation: "Return QueryKind::Highlights as default when no pattern matches in infer_query_kind()",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "a275d04", message: "feat(config): add infer_query_kind function for filename-based type inference", phase: "green" }],
        notes: ["Combined with subtask 2 since both were implemented in same function"],
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

  completed: [
    { number: 147, pbi_id: "PBI-174", goal: "Audit API visibility in LanguageCoordinator - 1 method made private", status: "done", subtasks: [] },
    { number: 146, pbi_id: "PBI-173", goal: "Parameterize offset clamping tests with rstest (3→1 test)", status: "done", subtasks: [] },
    { number: 145, pbi_id: "PBI-172", goal: "Relocate smoke tests from integration to unit test location", status: "done", subtasks: [] },
    { number: 144, pbi_id: "PBI-171", goal: "Investigate $/cancelRequest handling via custom_method - blocked by tower-lsp architecture", status: "cancelled", subtasks: [] },
    { number: 143, pbi_id: "PBI-170", goal: "Investigate $/cancelRequest - deferred (tower-lsp limitation, YAGNI)", status: "cancelled", subtasks: [] },
    { number: 142, pbi_id: "PBI-169", goal: "Fix bridge bookkeeping memory leak after crashes/restarts", status: "done", subtasks: [] },
    { number: 141, pbi_id: "PBI-168", goal: "Fix concurrent parse crash recovery to correctly identify failing parsers", status: "done", subtasks: [] },
  ],

  retrospectives: [
    { sprint: 147, improvements: [
      { action: "Test review findings (review-tests.md) addressed: smoke tests relocated, tests parameterized, API visibility audited", timing: "immediate", status: "completed", outcome: "3 PBIs completed (172-174), test pyramid improved, rstest adopted for parameterization" },
    ] },
    { sprint: 144, improvements: [
      { action: "Investigation: LspServiceBuilder.custom_method cannot intercept $/cancelRequest because tower-lsp registers it first in generated code before custom methods", timing: "product", status: "completed", outcome: "PBI-171 deferred - tower-lsp's Router uses HashMap with first-registration-wins, blocking custom interception" },
      { action: "Current architecture already supports request superseding: new semantic token requests automatically cancel previous ones via SemanticRequestTracker", timing: "product", status: "completed", outcome: "Explicit $/cancelRequest handling deemed unnecessary (YAGNI) - existing superseding mechanism sufficient for user typing scenarios" },
    ] },
    { sprint: 143, improvements: [
      { action: "Review-codex3 findings: PBI-168, PBI-169 fixed; PBI-170 deferred (tower-lsp limitation, YAGNI)", timing: "product", status: "completed", outcome: "2/3 issues resolved, 1 deferred" },
    ] },
    { sprint: 140, improvements: [
      { action: "Flaky tests eliminated with serial_test for rust-analyzer tests", timing: "immediate", status: "completed", outcome: "373/373 tests pass consistently (10 consecutive runs verified)" },
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
