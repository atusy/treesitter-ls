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
    // ADR-0010 Implementation: Configuration Merging Strategy
    // Completed: PBI-151 (Sprint 118), PBI-150 (Sprint 119)

    {
      id: "PBI-149",
      story: {
        role: "treesitter-ls user managing configurations",
        capability: "set my preferred editor defaults in a single user config file",
        benefit: "my settings apply across all projects without duplicating configuration",
      },
      acceptance_criteria: [
        { criterion: "User config loads from $XDG_CONFIG_HOME/treesitter-ls/treesitter-ls.toml", verification: "Unit test: config path resolves with XDG_CONFIG_HOME set" },
        { criterion: "Falls back to ~/.config/treesitter-ls/treesitter-ls.toml when XDG unset", verification: "Unit test: fallback path when XDG not set" },
        { criterion: "Missing user config silently ignored (zero-config works)", verification: "Unit test: no error when file missing" },
        { criterion: "Invalid user config causes startup failure with clear error", verification: "Unit test: parse error produces descriptive message" },
      ],
      status: "ready",
    },
  ],

  sprint: {
    number: 120,
    pbi_id: "PBI-149",
    goal: "Enable users to set editor-wide defaults in a user config file at XDG standard location",
    status: "in_progress",
    subtasks: [
      {
        test: "Test user_config_path() returns $XDG_CONFIG_HOME/treesitter-ls/treesitter-ls.toml when XDG_CONFIG_HOME is set",
        implementation: "Implement user_config_path() function that reads XDG_CONFIG_HOME env var and appends treesitter-ls/treesitter-ls.toml",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "40820bf", message: "feat(config): add user_config_path() with XDG_CONFIG_HOME support", phase: "green" }],
        notes: [],
      },
      {
        test: "Test user_config_path() returns ~/.config/treesitter-ls/treesitter-ls.toml when XDG_CONFIG_HOME is unset",
        implementation: "Add fallback logic to user_config_path() using dirs::config_dir() or home_dir() with .config appended",
        type: "behavioral",
        status: "green",
        commits: [],
        notes: [],
      },
      {
        test: "Test load_user_config() returns None (or default Config) when user config file does not exist",
        implementation: "Implement load_user_config() that returns Ok(None) for missing file - zero-config experience preserved",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
      },
      {
        test: "Test load_user_config() returns descriptive ConfigError when user config file exists but contains invalid TOML or schema",
        implementation: "Add error handling that wraps TOML parse errors with context including file path and error location",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
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
    { number: 119, pbi_id: "PBI-150", goal: "Enable users to inherit settings from user config to project config without duplication via merge_all()", status: "done", subtasks: [] },
    { number: 118, pbi_id: "PBI-151", goal: "Enable unified query configuration with queries array and type inference from filename patterns", status: "done", subtasks: [] },
  ],

  retrospectives: [
    { sprint: 147, improvements: [
      { action: "Test review findings (review-tests.md) addressed: smoke tests relocated, tests parameterized, API visibility audited", timing: "immediate", status: "completed", outcome: "3 PBIs completed (172-174), test pyramid improved, rstest adopted for parameterization" },
    ] },
    { sprint: 119, improvements: [
      { action: "Deep merge HashMap pattern (entry().and_modify().or_insert()) is reusable - extract to generic helper when 3rd usage appears", timing: "sprint", status: "active", outcome: null },
      { action: "Empty Vec fields should inherit from fallback layer - Vec merge uses .is_empty() check, not Option semantics", timing: "immediate", status: "completed", outcome: "merge_language_servers: if !primary.cmd.is_empty() condition (lines 249, 252)" },
      { action: "E2E test failures unrelated to PBI indicate technical debt - add test_lsp_document_highlight to product backlog", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 118, improvements: [
      { action: "Combined subtasks indicate shared implementation - consider merging during planning when default behavior is intrinsic to core function", timing: "immediate", status: "completed", outcome: "Subtasks 2 and 3 merged: infer_query_kind() includes default in a275d04" },
      { action: "New public types exported via config.rs need explicit pub - apply YAGNI-pub: verify each pub is needed", timing: "immediate", status: "completed", outcome: "QueryKind, QueryItem, infer_query_kind exported in config.rs for external use" },
      { action: "Document backward compatibility decisions during planning - not mid-sprint", timing: "sprint", status: "active", outcome: null },
      { action: "Investigate transient E2E markdown loading failures - may indicate timing issues in test setup", timing: "product", status: "active", outcome: null },
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
